// makefun-site's Markdown (lib/markdown.js): its own subset, lexed into block
// tokens and then rendered. Ported faithfully, including the inline scanner's
// escape-aware search and every block boundary rule.

use crate::util::{escape_char, is_escapable, is_js_ws, push_escaped, slugify, unique_slug};
use std::collections::BTreeSet;

fn is_blank(line: &str) -> bool {
    line.bytes().all(|c| c == b' ' || c == b'\t')
}

/// A fence-open line: three backticks, an optional language
/// ([A-Za-z0-9_+-]*), then spaces or tabs only. Returns the language.
fn fence_open(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("```")?;
    let mut k = 0;
    let b = rest.as_bytes();
    while k < b.len()
        && (b[k].is_ascii_alphanumeric() || b[k] == b'_' || b[k] == b'+' || b[k] == b'-')
    {
        k += 1;
    }
    if rest[k..].bytes().all(|c| c == b' ' || c == b'\t') {
        Some(&rest[..k])
    } else {
        None
    }
}

fn fence_close(line: &str) -> bool {
    match line.strip_prefix("```") {
        Some(rest) => rest.bytes().all(|c| c == b' ' || c == b'\t'),
        None => false,
    }
}

/// A heading line's level and text, or None (a heading's text is never empty).
fn heading_of(line: &str) -> Option<(usize, &str)> {
    let b = line.as_bytes();
    let mut hashes = 0;
    while hashes < b.len() && b[hashes] == b'#' {
        hashes += 1;
    }
    if hashes == 0 || hashes > 6 {
        return None;
    }
    if hashes >= b.len() || (b[hashes] != b' ' && b[hashes] != b'\t') {
        return None;
    }
    let rest = &line[hashes..];
    let rest = rest.trim_start_matches([' ', '\t']);
    let text = rest.trim_end_matches([' ', '\t']);
    if text.is_empty() {
        None
    } else {
        Some((hashes, text))
    }
}

fn is_hr(line: &str) -> bool {
    let t = line.trim_end_matches([' ', '\t']);
    t == "---" || t == "***"
}

/// A bullet list line: `- ` or `* `, returning the item's text.
fn bullet_of(line: &str) -> Option<&str> {
    let b = line.as_bytes();
    if b.len() >= 2 && (b[0] == b'-' || b[0] == b'*') && b[1] == b' ' {
        Some(&line[2..])
    } else {
        None
    }
}

/// An ordered list line: digits and `. `, returning (number, item text).
fn ordered_of(line: &str) -> Option<(u64, &str)> {
    let b = line.as_bytes();
    let mut k = 0;
    while k < b.len() && b[k].is_ascii_digit() {
        k += 1;
    }
    if k == 0 || k > 9 {
        return None;
    }
    if k + 1 < b.len() && b[k] == b'.' && b[k + 1] == b' ' {
        let n: u64 = line[..k].parse().ok()?;
        Some((n, &line[k + 2..]))
    } else {
        None
    }
}

/// A continuation line: two or more spaces then a non-space (JS \S).
fn is_continued(line: &str) -> bool {
    let b = line.as_bytes();
    let mut k = 0;
    while k < b.len() && b[k] == b' ' {
        k += 1;
    }
    if k < 2 {
        return false;
    }
    match line[k..].chars().next() {
        Some(c) => !is_js_ws(c),
        None => false,
    }
}

/// A line trimmed of spaces and tabs at both ends (markdown.js trimLine).
fn trim_line(line: &str) -> &str {
    line.trim_matches([' ', '\t'])
}

/// Whether `line` ends a paragraph: a blank line, a fence, a heading or a
/// thematic break.
fn ends_paragraph(line: &str) -> bool {
    is_blank(line) || fence_open(line).is_some() || heading_of(line).is_some() || is_hr(line)
}

enum Token<'a> {
    Code { lang: &'a str, lines: Vec<&'a str> },
    Heading { depth: usize, text: &'a str },
    Hr,
    Blockquote { lines: Vec<&'a str> },
    List { ordered: bool, start: u64, items: Vec<String> },
    Paragraph { text: String },
}

/// `src`'s block tokens, in order.
fn lex(src: &str) -> Vec<Token<'_>> {
    let lines: Vec<&str> = src.split('\n').collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if is_blank(line) {
            i += 1;
            continue;
        }
        if let Some(lang) = fence_open(line) {
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !fence_close(lines[i]) {
                body.push(lines[i]);
                i += 1;
            }
            i += 1;
            tokens.push(Token::Code { lang, lines: body });
            continue;
        }
        if let Some((depth, text)) = heading_of(line) {
            tokens.push(Token::Heading { depth, text });
            i += 1;
            continue;
        }
        if is_hr(line) {
            tokens.push(Token::Hr);
            i += 1;
            continue;
        }
        if line.starts_with('>') {
            let mut body = Vec::new();
            while i < lines.len() && lines[i].starts_with('>') {
                let l = lines[i];
                body.push(l.strip_prefix("> ").unwrap_or_else(|| l.strip_prefix('>').unwrap()));
                i += 1;
            }
            let lines: Vec<&str> = body
                .iter()
                .map(|l| trim_line(l))
                .filter(|l| !l.is_empty())
                .collect();
            tokens.push(Token::Blockquote { lines });
            continue;
        }
        let bullet = bullet_of(line);
        let ordered = if bullet.is_none() {
            ordered_of(line)
        } else {
            None
        };
        if bullet.is_some() || ordered.is_some() {
            let is_ordered = ordered.is_some();
            let mut items: Vec<Vec<&str>> = Vec::new();
            while i < lines.len() {
                if is_ordered {
                    match ordered_of(lines[i]) {
                        Some((_, text)) => {
                            items.push(vec![trim_line(text)]);
                            i += 1;
                        }
                        None => {
                            if is_continued(lines[i]) {
                                items.last_mut().unwrap().push(trim_line(lines[i]));
                                i += 1;
                            } else {
                                break;
                            }
                        }
                    }
                } else {
                    match bullet_of(lines[i]) {
                        Some(text) => {
                            items.push(vec![trim_line(text)]);
                            i += 1;
                        }
                        None => {
                            if is_continued(lines[i]) {
                                items.last_mut().unwrap().push(trim_line(lines[i]));
                                i += 1;
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
            let start = if is_ordered { ordered.unwrap().0 } else { 1 };
            tokens.push(Token::List {
                ordered: is_ordered,
                start,
                items: items.iter().map(|p| p.join("\n")).collect(),
            });
            continue;
        }
        let mut body = vec![trim_line(line)];
        i += 1;
        while i < lines.len() && !ends_paragraph(lines[i]) {
            body.push(trim_line(lines[i]));
            i += 1;
        }
        tokens.push(Token::Paragraph { text: body.join("\n") });
    }
    tokens
}

/// The first byte index from `from` where `s` holds `what` outside a backslash
/// escape, or None (markdown.js findUnescaped).
fn find_unescaped(s: &str, what: &str, from: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut j = from;
    while j + what.len() <= s.len() {
        if b[j] == b'\\' {
            if let Some(n) = s[j + 1..].chars().next() {
                if is_escapable(n) {
                    j += 1 + n.len_utf8();
                    continue;
                }
            }
        }
        if s[j..].starts_with(what) {
            return Some(j);
        }
        j += s[j..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    None
}

/// The first single `*` from `from` (one whose run is not `**`), outside an
/// escape, or None (markdown.js findSingleStar).
fn find_single_star(s: &str, from: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut j = from;
    while j < s.len() {
        let c = b[j];
        if c == b'\\' {
            if let Some(n) = s[j + 1..].chars().next() {
                if is_escapable(n) {
                    j += 1 + n.len_utf8();
                    continue;
                }
            }
        }
        if c != b'*' {
            j += s[j..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            continue;
        }
        if j + 1 < s.len() && b[j + 1] == b'*' {
            j += 2;
            continue;
        }
        return Some(j);
    }
    None
}

/// What a link resolves to. Implemented by the build (index.js markdown env).
pub trait Linker {
    fn link(&mut self, target: &str) -> String;
}

/// `s` rendered inline; `env.link(target)` is a link's href.
pub fn inline(s: &str, env: &mut dyn Linker) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < s.len() {
        let ch = s[i..].chars().next().unwrap();
        let clen = ch.len_utf8();
        if ch == '\\' {
            if let Some(n) = s[i + clen..].chars().next() {
                if is_escapable(n) {
                    let esc = escape_char(n);
                    if esc.is_empty() {
                        out.push(n);
                    } else {
                        out.push_str(esc);
                    }
                    i += clen + n.len_utf8();
                    continue;
                }
            }
        }
        if ch == '`' {
            let mut j = i + 1;
            let mut found = None;
            while j < s.len() {
                if b[j] == b'`' {
                    found = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(j) = found {
                if j > i + 1 {
                    out.push_str("<code>");
                    push_escaped(&mut out, &s[i + 1..j]);
                    out.push_str("</code>");
                    i = j + 1;
                    continue;
                }
            }
            out.push('`');
            i += 1;
            continue;
        }
        if ch == '*' {
            if i + 1 < s.len() && b[i + 1] == b'*' {
                if let Some(j) = find_unescaped(s, "**", i + 2) {
                    if j > i + 2 {
                        out.push_str("<strong>");
                        out.push_str(&inline(&s[i + 2..j], env));
                        out.push_str("</strong>");
                        i = j + 2;
                        continue;
                    }
                }
                out.push_str("**");
                i += 2;
                continue;
            }
            if let Some(j) = find_single_star(s, i + 1) {
                if j > i + 1 {
                    out.push_str("<em>");
                    out.push_str(&inline(&s[i + 1..j], env));
                    out.push_str("</em>");
                    i = j + 1;
                    continue;
                }
            }
            out.push('*');
            i += 1;
            continue;
        }
        if ch == '[' {
            let k = find_unescaped(s, "]", i + 1);
            let m = match k {
                Some(k) if k + 1 < s.len() && b[k + 1] == b'(' => {
                    let mut p = k + 2;
                    let mut found = None;
                    while p < s.len() {
                        if b[p] == b')' {
                            found = Some(p);
                            break;
                        }
                        p += 1;
                    }
                    found
                }
                _ => None,
            };
            let target = match (k, m) {
                (Some(_), Some(m)) => &s[k.unwrap() + 2..m],
                _ => "",
            };
            if !target.is_empty() && !target.chars().any(is_js_ws) {
                let href = env.link(target);
                out.push_str("<a href=\"");
                push_escaped(&mut out, &href);
                out.push_str("\">");
                out.push_str(&inline(&s[i + 1..k.unwrap()], env));
                out.push_str("</a>");
                i = m.unwrap() + 1;
                continue;
            }
            out.push('[');
            i += 1;
            continue;
        }
        let esc = escape_char(ch);
        if esc.is_empty() {
            out.push(ch);
        } else {
            out.push_str(esc);
        }
        i += clen;
    }
    out
}

pub struct TocEntry {
    pub level: usize,
    pub id: String,
    pub html: String,
}

pub struct Rendered {
    pub html: String,
    pub toc: Vec<TocEntry>,
    pub excerpt: String,
}

/// `src` rendered: its HTML (each block on its own lines, a "\n" after the
/// last), its table of contents (every level 2 and 3 heading) and its excerpt
/// (the first paragraph's inline HTML, or "").
pub fn render_markdown(src: &str, env: &mut dyn Linker) -> Rendered {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    let mut blocks: Vec<String> = Vec::new();
    let mut toc: Vec<TocEntry> = Vec::new();
    let mut excerpt: Option<String> = None;
    for t in lex(src) {
        match t {
            Token::Heading { depth, text } => {
                let id = unique_slug(&slugify(text), &mut ids);
                let html = inline(text, env);
                blocks.push(format!("<h{depth} id=\"{id}\">{html}</h{depth}>"));
                if depth == 2 || depth == 3 {
                    toc.push(TocEntry { level: depth, id, html });
                }
            }
            Token::Code { lang, lines } => {
                let cls = if lang.is_empty() {
                    String::new()
                } else {
                    let mut c = String::from(" class=\"language-");
                    push_escaped(&mut c, lang);
                    c.push('"');
                    c
                };
                let mut body = String::new();
                for l in lines {
                    push_escaped(&mut body, l);
                    body.push('\n');
                }
                blocks.push(format!("<pre><code{cls}>{body}</code></pre>"));
            }
            Token::Hr => blocks.push("<hr>".to_string()),
            Token::Blockquote { lines } => {
                if lines.is_empty() {
                    blocks.push("<blockquote>\n</blockquote>".to_string());
                } else {
                    let inner = inline(&lines.join("\n"), env);
                    blocks.push(format!("<blockquote>\n<p>{inner}</p>\n</blockquote>"));
                }
            }
            Token::List { ordered, start, items } => {
                let tag = if ordered { "ol" } else { "ul" };
                let open = if ordered && start != 1 {
                    format!("<ol start=\"{start}\">")
                } else {
                    format!("<{tag}>")
                };
                let mut block = String::new();
                block.push_str(&open);
                block.push('\n');
                for item in &items {
                    block.push_str("<li>");
                    block.push_str(&inline(item, env));
                    block.push_str("</li>\n");
                }
                block.push_str(&format!("</{tag}>"));
                blocks.push(block);
            }
            Token::Paragraph { text } => {
                let html = inline(&text, env);
                if excerpt.is_none() {
                    excerpt = Some(html.clone());
                }
                blocks.push(format!("<p>{html}</p>"));
            }
        }
    }
    let mut html = String::new();
    for b in blocks {
        html.push_str(&b);
        html.push('\n');
    }
    Rendered { html, toc, excerpt: excerpt.unwrap_or_default() }
}
