// site-generator-wasm: makefun-site 1.0.0's semantics, reimplemented in Rust
// and compiled to WebAssembly. The reference is the baseline's code
// (kit/baselines/makefun-site/vendor/makefun-site/); this file is an original
// implementation of the same pipeline, written from the README and the code's
// observable behaviour. No baseline code is copied.
//
// The entry point the harness uses is main.ts -> solve.js -> this module:
// solve.js hands the raw /input/cases.json bytes to `wasm_run_all`, which
// parses them, builds every case and returns the answers JSON bytes, so no
// JavaScript JSON.parse/stringify of the big payloads ever runs.

mod frontmatter;
mod markdown;
mod template;
mod util;

use frontmatter::{parse_front_matter, FmData};
use markdown::{render_markdown, Linker, TocEntry};
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use template::{parse_template, render_nodes, Ctx, Scope, Val};
use util::{
    cmp_u16, dir_of, js_to_integer, long_date, resolve_path, sanitize_lone_surrogates, slugify,
    unique_slug, words_of,
};

/// One case as the harness hands it: {"id", "input"}.
#[derive(Deserialize)]
struct CaseFile {
    #[allow(dead_code)]
    id: String,
    input: SiteInput,
}

#[derive(Deserialize, Default)]
struct Site {
    #[serde(default)]
    title: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    per_page: f64,
    #[serde(default)]
    recent: f64,
}

#[derive(Deserialize, Default)]
struct SiteInput {
    #[serde(default)]
    site: Site,
    #[serde(default)]
    templates: BTreeMap<String, String>,
    #[serde(default)]
    files: Vec<FileIn>,
}

#[derive(Deserialize, Default)]
struct FileIn {
    #[serde(default)]
    path: String,
    #[serde(default)]
    content: String,
}

/// A source that survived front matter and the checks.
struct Source {
    path: String,
    data: FmData,
    body: String,
    post: bool,
    layout: String,
    slug: String,
    out_dir: String,
    url: String,
    html: String,
    toc: Vec<TocEntry>,
    excerpt: String,
    tag_refs: Vec<TagRef>,
    newer: Option<usize>,
    older: Option<usize>,
}

#[derive(Clone)]
struct TagRef {
    name: String,
    slug: String,
    url: String,
}

struct Tag {
    name: String,
    slug: String,
    url: String,
    posts: Vec<usize>,
}

/// The top-level directories the generator writes itself.
const RESERVED: [&str; 3] = ["posts", "tags", "page"];

fn has_scheme(t: &str) -> bool {
    let b = t.as_bytes();
    if b.is_empty() || !b[0].is_ascii_alphabetic() {
        return false;
    }
    let mut i = 1;
    while i < b.len()
        && (b[i].is_ascii_alphanumeric() || b[i] == b'+' || b[i] == b'-' || b[i] == b'.')
    {
        i += 1;
    }
    i < b.len() && b[i] == b':'
}

struct MdLinker<'a> {
    dir: String,
    source_path: &'a str,
    by_source: &'a BTreeMap<String, String>,
    static_paths: &'a BTreeSet<String>,
    base: &'a str,
    broken: &'a mut BTreeSet<String>,
}

impl Linker for MdLinker<'_> {
    fn link(&mut self, target: &str) -> String {
        if has_scheme(target) || target.starts_with('/') || target.starts_with('#') {
            return target.to_string();
        }
        let hash = target.find('#');
        let (rel, frag) = match hash {
            Some(h) => (&target[..h], &target[h..]),
            None => (target, ""),
        };
        let path = if rel.is_empty() {
            None
        } else {
            resolve_path(&self.dir, rel)
        };
        if let Some(p) = &path {
            if let Some(url) = self.by_source.get(p) {
                return format!("{url}{frag}");
            }
            if self.static_paths.contains(p) {
                return format!("{}/{p}{frag}", self.base);
            }
        }
        self.broken
            .insert(format!("broken-link {} -> {target}", self.source_path));
        target.to_string()
    }
}

/// A post's card, as lists show it (index.js cardOf).
fn card_fields(s: &Source) -> Vec<(String, Val)> {
    vec![
        ("title".to_string(), Val::Str(s.data.title.clone())),
        ("url".to_string(), Val::Str(s.url.clone())),
        (
            "date".to_string(),
            Val::Str(s.data.date.clone().unwrap_or_default()),
        ),
        (
            "date_long".to_string(),
            Val::Str(
                s.data
                    .date
                    .as_ref()
                    .map(|d| long_date(d))
                    .unwrap_or_default(),
            ),
        ),
        (
            "summary".to_string(),
            Val::Str(s.data.summary.clone().unwrap_or_default()),
        ),
        ("excerpt".to_string(), Val::Str(s.excerpt.clone())),
        (
            "reading_minutes".to_string(),
            Val::Num(((words_of(&s.body) as f64 / 200.0).ceil() as i64).max(1)),
        ),
        (
            "tags".to_string(),
            Val::Arr(
                s.tag_refs
                    .iter()
                    .map(|t| {
                        Val::Obj(vec![
                            ("name".to_string(), Val::Str(t.name.clone())),
                            ("slug".to_string(), Val::Str(t.slug.clone())),
                            ("url".to_string(), Val::Str(t.url.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]
}

fn link_val(s: Option<&Source>) -> Val {
    match s {
        None => Val::Null,
        Some(s) => Val::Obj(vec![
            ("title".to_string(), Val::Str(s.data.title.clone())),
            ("url".to_string(), Val::Str(s.url.clone())),
            (
                "date".to_string(),
                Val::Str(s.data.date.clone().unwrap_or_default()),
            ),
            (
                "date_long".to_string(),
                Val::Str(
                    s.data
                        .date
                        .as_ref()
                        .map(|d| long_date(d))
                        .unwrap_or_default(),
                ),
            ),
        ]),
    }
}

/// A page as its layout reads it (index.js pageOf).
fn page_val(s: &Source, sources: &[Source]) -> Val {
    let mut fields = card_fields(s);
    fields.push(("slug".to_string(), Val::Str(s.slug.clone())));
    fields.push(("content".to_string(), Val::Str(s.html.clone())));
    fields.push((
        "toc".to_string(),
        Val::Arr(
            s.toc
                .iter()
                .map(|h| {
                    Val::Obj(vec![
                        ("level".to_string(), Val::Num(h.level as i64)),
                        ("id".to_string(), Val::Str(h.id.clone())),
                        ("html".to_string(), Val::Str(h.html.clone())),
                    ])
                })
                .collect(),
        ),
    ));
    fields.push((
        "newer".to_string(),
        link_val(s.newer.map(|i| &sources[i])),
    ));
    fields.push((
        "older".to_string(),
        link_val(s.older.map(|i| &sources[i])),
    ));
    fields.push(("is_post".to_string(), Val::Bool(s.post)));
    Val::Obj(fields)
}

fn render_named(
    ctx: &Ctx,
    site: &Val,
    tags: &Val,
    recent: &Val,
    name: &str,
    extra: Vec<(String, Val)>,
) -> Option<String> {
    let nodes = ctx.templates.get(name)?;
    let mut vars = vec![
        ("site".to_string(), site.clone()),
        ("tags".to_string(), tags.clone()),
        ("recent".to_string(), recent.clone()),
    ];
    vars.extend(extra);
    let scope = Scope::root(vars);
    let mut out = String::new();
    render_nodes(nodes, &scope, ctx, 0, &mut out);
    Some(out)
}

/// One site's output tree: path -> text (index.js build).
fn build(input: &SiteInput) -> BTreeMap<String, String> {
    // Templates: parsed in code-unit order of name. One that does not parse
    // is reported and left out.
    let mut ctx = Ctx { templates: BTreeMap::new() };
    let mut report: Vec<String> = Vec::new();
    let mut names: Vec<&String> = input.templates.keys().collect();
    names.sort_by(|a, b| cmp_u16(a, b));
    for name in names {
        match parse_template(name, &input.templates[name]) {
            Ok(nodes) => {
                ctx.templates.insert(name.clone(), nodes);
            }
            Err(msg) => report.push(msg),
        }
    }
    let base = input.site.base_url.trim_end_matches('/').to_string();

    // Sources: in code-unit order of path, line ends made "\n".
    let mut sorted_files: Vec<&FileIn> = input.files.iter().collect();
    sorted_files.sort_by(|a, b| cmp_u16(&a.path, &b.path));
    let mut sources: Vec<Source> = Vec::new();
    let mut statics: Vec<&FileIn> = Vec::new();
    for f in sorted_files {
        if f.path.ends_with(".md") {
            let text = f.content.replace("\r\n", "\n").replace('\r', "\n");
            sources.push(Source {
                path: f.path.clone(),
                data: FmData::default(),
                body: text,
                post: false,
                layout: String::new(),
                slug: String::new(),
                out_dir: String::new(),
                url: String::new(),
                html: String::new(),
                toc: Vec::new(),
                excerpt: String::new(),
                tag_refs: Vec::new(),
                newer: None,
                older: None,
            });
        } else {
            statics.push(f);
        }
    }

    // Front matter and the checks, in order: refused, reserved directory,
    // post with no date, layout missing, draft.
    let mut kept: Vec<Source> = Vec::new();
    for mut s in sources {
        let fm = parse_front_matter(&s.body);
        let (data, body) = match fm {
            Ok(v) => v,
            Err(e) => {
                report.push(format!("refused {}: {e}", s.path));
                continue;
            }
        };
        s.data = data;
        s.body = body;
        s.post = s.path.starts_with("posts/");
        let own = RESERVED
            .iter()
            .find(|d| **d != "posts" && s.path.starts_with(&format!("{d}/")));
        if let Some(d) = own {
            report.push(format!("refused {}: {d}/ is the generator's own", s.path));
            continue;
        }
        if s.post && s.data.date.is_none() {
            report.push(format!("refused {}: a post needs a date", s.path));
            continue;
        }
        let layout = s
            .data
            .layout
            .clone()
            .unwrap_or_else(|| if s.post { "post".to_string() } else { "page".to_string() });
        if !ctx.templates.contains_key(&layout) {
            report.push(format!(
                "refused {}: layout \"{layout}\" is no template that parses",
                s.path
            ));
            continue;
        }
        s.layout = layout;
        if s.data.draft {
            report.push(format!("draft {}", s.path));
            continue;
        }
        kept.push(s);
    }
    let mut sources = kept;

    // Permalinks: each page's directory and url, its slug unique in its
    // directory in source order.
    let mut taken: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut by_source: BTreeMap<String, String> = BTreeMap::new();
    for s in sources.iter_mut() {
        let dir = if s.post {
            "posts".to_string()
        } else {
            dir_of(&s.path).to_string()
        };
        let taken_in = taken.entry(dir.clone()).or_insert_with(|| {
            if dir.is_empty() {
                RESERVED.iter().map(|d| d.to_string()).collect()
            } else {
                BTreeSet::new()
            }
        });
        let slug = unique_slug(
            &s.data
                .slug
                .clone()
                .unwrap_or_else(|| slugify(&s.data.title)),
            taken_in,
        );
        s.slug = slug;
        s.out_dir = if dir.is_empty() {
            s.slug.clone()
        } else {
            format!("{dir}/{}", s.slug)
        };
        s.url = format!("{}/{}/", base, s.out_dir);
        by_source.insert(s.path.clone(), s.url.clone());
    }

    // Markdown: each source's body rendered to HTML, its links resolved.
    let static_paths: BTreeSet<String> = statics.iter().map(|f| f.path.clone()).collect();
    let mut broken: BTreeSet<String> = BTreeSet::new();
    for s in sources.iter_mut() {
        let dir = dir_of(&s.path).to_string();
        let mut linker = MdLinker {
            dir,
            source_path: &s.path,
            by_source: &by_source,
            static_paths: &static_paths,
            base: &base,
            broken: &mut broken,
        };
        let md = render_markdown(&s.body, &mut linker);
        s.html = md.html;
        s.toc = md.toc;
        s.excerpt = md.excerpt;
    }

    // Collections: the posts, newest date first, then by slug.
    let mut post_order: Vec<usize> = sources
        .iter()
        .enumerate()
        .filter(|(_, s)| s.post)
        .map(|(i, _)| i)
        .collect();
    post_order.sort_by(|&a, &b| {
        let x = &sources[a];
        let y = &sources[b];
        cmp_u16(
            y.data.date.as_deref().unwrap_or(""),
            x.data.date.as_deref().unwrap_or(""),
        )
        .then_with(|| cmp_u16(&x.slug, &y.slug))
    });
    for (k, &idx) in post_order.iter().enumerate() {
        sources[idx].newer = if k == 0 { None } else { Some(post_order[k - 1]) };
        sources[idx].older = if k + 1 == post_order.len() {
            None
        } else {
            Some(post_order[k + 1])
        };
    }

    // Tags, grouped by slug, named as first met walking the posts in
    // collection order, ordered by count then slug.
    let own_tags = |s: &Source| -> Vec<String> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut out = Vec::new();
        for name in &s.data.tags {
            let slug = slugify(name);
            if seen.insert(slug) {
                out.push(name.clone());
            }
        }
        out
    };
    let mut by_slug: BTreeMap<String, Tag> = BTreeMap::new();
    for &p in &post_order {
        for name in own_tags(&sources[p]) {
            let slug = slugify(&name);
            let tag = by_slug.entry(slug.clone()).or_insert_with(|| Tag {
                name: name.clone(),
                slug: slug.clone(),
                url: format!("{base}/tags/{slug}/"),
                posts: Vec::new(),
            });
            tag.posts.push(p);
            sources[p].tag_refs.push(TagRef {
                name: tag.name.clone(),
                slug: tag.slug.clone(),
                url: tag.url.clone(),
            });
        }
    }
    for s in sources.iter_mut() {
        if s.post {
            continue;
        }
        let mut refs = Vec::new();
        for name in own_tags(s) {
            let slug = slugify(&name);
            match by_slug.get(&slug) {
                Some(t) => refs.push(TagRef {
                    name: t.name.clone(),
                    slug: t.slug.clone(),
                    url: t.url.clone(),
                }),
                None => refs.push(TagRef { name, slug, url: String::new() }),
            }
        }
        s.tag_refs = refs;
    }
    let mut tags: Vec<&Tag> = by_slug.values().collect();
    tags.sort_by(|a, b| {
        b.posts
            .len()
            .cmp(&a.posts.len())
            .then_with(|| cmp_u16(&a.slug, &b.slug))
    });

    // Layouts.
    let cards: Vec<Val> = sources
        .iter()
        .map(|s| {
            if s.post {
                Val::Obj(card_fields(s))
            } else {
                Val::Null
            }
        })
        .collect();
    let site_val = Val::Obj(vec![
        ("title".to_string(), Val::Str(input.site.title.clone())),
        ("base_url".to_string(), Val::Str(base.clone())),
    ]);
    let tags_val = Val::Arr(
        tags.iter()
            .map(|t| {
                Val::Obj(vec![
                    ("name".to_string(), Val::Str(t.name.clone())),
                    ("slug".to_string(), Val::Str(t.slug.clone())),
                    ("url".to_string(), Val::Str(t.url.clone())),
                    ("count".to_string(), Val::Num(t.posts.len() as i64)),
                ])
            })
            .collect(),
    );
    let recent_n = js_to_integer(input.site.recent);
    let recent_n = if recent_n < 0 {
        0
    } else {
        (recent_n as usize).min(post_order.len())
    };
    let recent_val = Val::Arr(
        post_order[..recent_n]
            .iter()
            .map(|&i| cards[i].clone())
            .collect(),
    );

    let mut output: BTreeMap<String, String> = BTreeMap::new();
    for s in sources.iter() {
        if let Some(html) = render_named(
            &ctx,
            &site_val,
            &tags_val,
            &recent_val,
            &s.layout,
            vec![("page".to_string(), page_val(s, &sources))],
        ) {
            output.insert(format!("{}/index.html", s.out_dir), html);
        }
    }
    let per = input.site.per_page;
    let pages = (post_order.len() as f64 / per).ceil().max(1.0);
    let index_url = |k: usize| -> String {
        if k == 1 {
            format!("{base}/")
        } else {
            format!("{base}/page/{k}/")
        }
    };
    let mut k: usize = 1;
    while (k as f64) <= pages {
        let start = js_to_integer((k - 1) as f64 * per);
        let end = js_to_integer(k as f64 * per);
        let start = (start.max(0) as usize).min(post_order.len());
        let end = (end.max(0) as usize).min(post_order.len());
        let posts_val = Val::Arr(
            post_order[start..end]
                .iter()
                .map(|&i| cards[i].clone())
                .collect(),
        );
        let total = pages as i64;
        let pager_val = Val::Obj(vec![
            ("number".to_string(), Val::Num(k as i64)),
            ("total".to_string(), Val::Num(total)),
            ("url".to_string(), Val::Str(index_url(k))),
            (
                "newer_url".to_string(),
                Val::Str(if k == 1 {
                    String::new()
                } else {
                    index_url(k - 1)
                }),
            ),
            (
                "older_url".to_string(),
                Val::Str(if (k as f64) == pages {
                    String::new()
                } else {
                    index_url(k + 1)
                }),
            ),
        ]);
        if let Some(html) = render_named(
            &ctx,
            &site_val,
            &tags_val,
            &recent_val,
            "index",
            vec![
                ("posts".to_string(), posts_val),
                ("pager".to_string(), pager_val),
            ],
        ) {
            let path = if k == 1 {
                "index.html".to_string()
            } else {
                format!("page/{k}/index.html")
            };
            output.insert(path, html);
        }
        k += 1;
    }
    for t in &tags {
        let tag_val = Val::Obj(vec![
            ("name".to_string(), Val::Str(t.name.clone())),
            ("slug".to_string(), Val::Str(t.slug.clone())),
            ("url".to_string(), Val::Str(t.url.clone())),
            ("count".to_string(), Val::Num(t.posts.len() as i64)),
            (
                "posts".to_string(),
                Val::Arr(t.posts.iter().map(|&i| cards[i].clone()).collect()),
            ),
        ]);
        if let Some(html) = render_named(
            &ctx,
            &site_val,
            &tags_val,
            &recent_val,
            "tag",
            vec![("tag".to_string(), tag_val)],
        ) {
            output.insert(format!("tags/{}/index.html", t.slug), html);
        }
    }
    if let Some(html) = render_named(&ctx, &site_val, &tags_val, &recent_val, "tags", vec![]) {
        output.insert("tags/index.html".to_string(), html);
    }
    for f in &statics {
        // A static file named exactly "__proto__" is dropped by the reference:
        // assigning it to a plain object sets the prototype instead of an own
        // property, so it never appears in the output.
        if f.path == "__proto__" {
            continue;
        }
        if output.contains_key(&f.path) {
            report.push(format!("shadowed {}: a built page has its path", f.path));
        } else {
            output.insert(f.path.clone(), f.content.clone());
        }
    }

    // The report.
    let mut lines: Vec<String> = report;
    lines.extend(broken.into_iter());
    lines.sort_by(|a, b| cmp_u16(a, b));
    let pages_n = sources.iter().filter(|s| !s.post).count();
    lines.push(format!(
        "built {pages_n} pages, {} posts, {} tags and {} static files",
        post_order.len(),
        tags.len(),
        statics.len()
    ));
    let mut report_text = lines.join("\n");
    report_text.push('\n');
    output.insert("build-report.txt".to_string(), report_text);
    output
}

/// Parse the cases JSON and build every case, returning the answers JSON.
fn run_all_bytes(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut bytes = input.to_vec();
    sanitize_lone_surrogates(&mut bytes);
    let text = String::from_utf8_lossy(&bytes);
    let cases: Vec<CaseFile> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut answers: Vec<BTreeMap<String, String>> = Vec::with_capacity(cases.len());
    for c in &cases {
        answers.push(build(&c.input));
    }
    serde_json::to_vec(&answers).map_err(|e| e.to_string())
}

/// Parse one site input and build it, returning its answer JSON.
fn run_one_bytes(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut bytes = input.to_vec();
    sanitize_lone_surrogates(&mut bytes);
    let text = String::from_utf8_lossy(&bytes);
    let site: SiteInput = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let answer = build(&site);
    serde_json::to_vec(&answer).map_err(|e| e.to_string())
}

thread_local! {
    static IN_BUF: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static OUT_BUF: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

fn finish(res: Result<Vec<u8>, String>) -> u64 {
    match res {
        Ok(out) => OUT_BUF.with(|b| {
            let mut buf = b.borrow_mut();
            buf.clear();
            buf.extend_from_slice(&out);
            ((buf.len() as u64) << 32) | (buf.as_ptr() as u64)
        }),
        Err(_) => 0,
    }
}

/// A writable buffer for the input bytes: returns a pointer to `size` bytes
/// the caller fills, then calls `wasm_run_all` / `wasm_run_one`.
#[unsafe(no_mangle)]
pub extern "C" fn wasm_input_ptr(size: usize) -> *mut u8 {
    IN_BUF.with(|b| {
        let mut b = b.borrow_mut();
        b.resize(size, 0);
        b.as_mut_ptr()
    })
}

/// Build every case in the cases JSON at the input buffer; returns
/// (out_ptr | out_len << 32), or 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn wasm_run_all(len: usize) -> u64 {
    let res = IN_BUF.with(|b| {
        let input = b.borrow();
        let slice = if len <= input.len() { &input[..len] } else { &input[..] };
        run_all_bytes(slice)
    });
    finish(res)
}

/// Build one site input at the input buffer; returns (out_ptr | out_len <<
/// 32), or 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn wasm_run_one(len: usize) -> u64 {
    let res = IN_BUF.with(|b| {
        let input = b.borrow();
        let slice = if len <= input.len() { &input[..len] } else { &input[..] };
        run_one_bytes(slice)
    });
    finish(res)
}
