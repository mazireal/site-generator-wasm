// makefun-site's template dialect (lib/template.js): its own, not Liquid,
// Nunjucks, Mustache or EJS. Ported faithfully: tokenize (with the standalone
// tag line rule), the stack parser, scope-chain lookup and rendering.

use crate::util::{is_js_ws, js_json_string, js_num, js_trim, push_escaped};
use std::collections::BTreeMap;

/// A template value, as templates read it. Objects are field lists; lookup
/// takes the last field of a name (a later spread or a for's `loop` wins over
/// an earlier binding of the same name, as in the JavaScript objects).
#[derive(Debug, Clone)]
pub enum Val {
    Null,
    Bool(bool),
    Num(i64),
    Str(String),
    Arr(Vec<Val>),
    Obj(Vec<(String, Val)>),
}

impl Val {
    fn obj_get<'a>(fields: &'a [(String, Val)], name: &str) -> Option<&'a Val> {
        fields.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v)
    }
}

#[derive(Debug, Clone)]
pub enum Node {
    Text(String),
    Var(Vec<String>),
    Raw(Vec<String>),
    If { not: bool, path: Vec<String>, then: Vec<Node>, els: Option<Vec<Node>> },
    For { name: String, path: Vec<String>, body: Vec<Node> },
    Include(String),
}

enum Tok {
    Text(String),
    Var(Vec<String>),
    Raw(Vec<String>),
    Tag(String),
}

/// The path `text` names, split at its dots, or a TemplateError message.
fn path_of(name: &str, text: &str) -> Result<Vec<String>, String> {
    let ok = !text.is_empty()
        && text.split('.').all(|part| {
            let mut cs = part.chars();
            match cs.next() {
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
                _ => return false,
            }
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
    if ok {
        Ok(text.split('.').map(|s| s.to_string()).collect())
    } else {
        Err(format!(
            "template \"{name}\": {} is not a path",
            js_json_string(text)
        ))
    }
}

fn is_for_tag(tag: &str) -> Option<(String, String)> {
    // /^for ([A-Za-z_][A-Za-z0-9_]*) in (\S+)$/
    let rest = tag.strip_prefix("for ")?;
    let mut parts = rest.splitn(2, " in ");
    let name = parts.next()?;
    let path = parts.next()?;
    if path.chars().any(is_js_ws) {
        return None;
    }
    let mut cs = name.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return None,
    }
    if !cs.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    if name.is_empty() || path.is_empty() {
        return None;
    }
    Some((name.to_string(), path.to_string()))
}

fn is_if_tag(tag: &str) -> Option<(bool, String)> {
    // /^if (not )?(\S+)$/
    let rest = tag.strip_prefix("if ")?;
    let (not, path) = match rest.strip_prefix("not ") {
        Some(p) => (true, p),
        None => (false, rest),
    };
    if path.is_empty() || path.chars().any(is_js_ws) {
        return None;
    }
    Some((not, path.to_string()))
}

fn is_include_tag(tag: &str) -> Option<String> {
    // /^include "([a-z0-9-]+)"$/
    let inner = tag.strip_prefix("include \"")?.strip_suffix('"')?;
    if !inner.is_empty()
        && inner
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        Some(inner.to_string())
    } else {
        None
    }
}

/// `src`'s tokens: text, {{ }}, {{{ }}} and {% %}, a standalone tag's line
/// taken with it.
fn tokenize(name: &str, src: &str) -> Result<Vec<Tok>, String> {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let b = src.as_bytes();
    let mut i = 0;
    while i < src.len() {
        // Find the next '{'.
        let mut open: Option<usize> = None;
        let mut j = i;
        while j < src.len() {
            if b[j] == b'{' {
                open = Some(j);
                break;
            }
            j += 1;
        }
        let open = match open {
            Some(o) if o != src.len() - 1 => o,
            _ => {
                text.push_str(&src[i..]);
                break;
            }
        };
        let next = b[open + 1];
        if next != b'{' && next != b'%' {
            text.push_str(&src[i..open + 1]);
            i = open + 1;
            continue;
        }
        text.push_str(&src[i..open]);
        if next == b'{' {
            let raw = open + 2 < src.len() && b[open + 2] == b'{';
            let closer = if raw { "}}}" } else { "}}" };
            let from = open + if raw { 3 } else { 2 };
            let close = src[from..].find(closer).map(|k| k + from);
            let close = match close {
                Some(c) => c,
                None => {
                    return Err(format!(
                        "template \"{name}\": a {} is not closed",
                        if raw { "{{{" } else { "{{" }
                    ))
                }
            };
            if !text.is_empty() {
                tokens.push(Tok::Text(std::mem::take(&mut text)));
            }
            let path = path_of(name, js_trim(&src[from..close]))?;
            tokens.push(if raw { Tok::Raw(path) } else { Tok::Var(path) });
            i = close + if raw { 3 } else { 2 };
            continue;
        }
        // {% ... %}
        let close = src[open + 2..].find("%}").map(|k| k + open + 2);
        let close = match close {
            Some(c) => c,
            None => return Err(format!("template \"{name}\": a {{% is not closed")),
        };
        let end = close + 2;
        let line_start = src[..open].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let nl = src[end..].find('\n').map(|k| k + end);
        let line_end = nl.unwrap_or(src.len());
        let standalone = src[line_start..open]
            .bytes()
            .all(|c| c == b' ' || c == b'\t')
            && src[end..line_end]
                .bytes()
                .all(|c| c == b' ' || c == b'\t');
        if standalone {
            let drop = open - line_start;
            let new_len = text.len() - drop;
            text.truncate(new_len);
        }
        if !text.is_empty() {
            tokens.push(Tok::Text(std::mem::take(&mut text)));
        }
        tokens.push(Tok::Tag(js_trim(&src[open + 2..close]).to_string()));
        i = if standalone {
            match nl {
                Some(n) => n + 1,
                None => src.len(),
            }
        } else {
            end
        };
    }
    if !text.is_empty() {
        tokens.push(Tok::Text(text));
    }
    Ok(tokens)
}

/// `src` parsed into its tree of nodes (a TemplateError message when it does
/// not parse).
pub fn parse_template(name: &str, src: &str) -> Result<Vec<Node>, String> {
    let toks = tokenize(name, src)?;
    // Stack of (kind, node index path). We build with explicit node stacks:
    // each frame holds the node being built and which body is current.
    #[derive(Debug)]
    enum Frame {
        Root { body: Vec<Node> },
        For { name: String, path: Vec<String>, body: Vec<Node> },
        If { not: bool, path: Vec<String>, then: Vec<Node>, els: Option<Vec<Node>> },
    }
    impl Frame {
        fn body_mut(&mut self) -> &mut Vec<Node> {
            match self {
                Frame::Root { body } => body,
                Frame::For { body, .. } => body,
                Frame::If { then, els, .. } => match els {
                    Some(e) => e,
                    None => then,
                },
            }
        }
        fn kind(&self) -> &'static str {
            match self {
                Frame::Root { .. } => "root",
                Frame::For { .. } => "for",
                Frame::If { .. } => "if",
            }
        }
        fn into_node(self) -> Node {
            match self {
                Frame::Root { .. } => unreachable!(),
                Frame::For { name, path, body } => Node::For { name, path, body },
                Frame::If { not, path, then, els } => Node::If { not, path, then, els },
            }
        }
    }
    let mut stack: Vec<Frame> = vec![Frame::Root { body: Vec::new() }];
    for tok in toks {
        match tok {
            Tok::Text(t) => stack.last_mut().unwrap().body_mut().push(Node::Text(t)),
            Tok::Var(p) => stack.last_mut().unwrap().body_mut().push(Node::Var(p)),
            Tok::Raw(p) => stack.last_mut().unwrap().body_mut().push(Node::Raw(p)),
            Tok::Tag(tag) => {
                if let Some((fname, fpath)) = is_for_tag(&tag) {
                    let path = path_of(name, &fpath)?;
                    stack.push(Frame::For { name: fname, path, body: Vec::new() });
                } else if let Some((not, ipath)) = is_if_tag(&tag) {
                    let path = path_of(name, &ipath)?;
                    stack.push(Frame::If { not, path, then: Vec::new(), els: None });
                } else if tag == "else" {
                    let top = stack.last_mut().unwrap();
                    match top {
                        Frame::If { els, .. } if els.is_none() => {
                            *els = Some(Vec::new());
                        }
                        _ => {
                            return Err(format!(
                                "template \"{name}\": an else outside an if"
                            ))
                        }
                    }
                } else if tag == "endfor" || tag == "endif" {
                    let want = if tag == "endfor" { "for" } else { "if" };
                    let top_kind = stack.last().unwrap().kind();
                    if top_kind != want {
                        return Err(format!(
                            "template \"{name}\": an {tag} with no open {}",
                            &tag[3..]
                        ));
                    }
                    let frame = stack.pop().unwrap();
                    let node = frame.into_node();
                    stack.last_mut().unwrap().body_mut().push(node);
                } else if let Some(inc) = is_include_tag(&tag) {
                    stack.last_mut().unwrap().body_mut().push(Node::Include(inc));
                } else {
                    return Err(format!(
                        "template \"{name}\": {{% {tag} %}} is not a tag"
                    ));
                }
            }
        }
    }
    if stack.len() > 1 {
        let kind = stack.last().unwrap().kind();
        return Err(format!("template \"{name}\": a {kind} is not closed"));
    }
    match stack.pop().unwrap() {
        Frame::Root { body } => Ok(body),
        _ => unreachable!(),
    }
}

/// A scope: the vars bound here, and its parent. Lookup reads a path's first
/// name from the nearest scope holding it (a for's `loop` before its element),
/// then each next name as an own property of the value so far.
pub struct Scope<'a> {
    pub vars: Vec<(String, Val)>,
    pub parent: Option<&'a Scope<'a>>,
}

impl<'a> Scope<'a> {
    pub fn root(vars: Vec<(String, Val)>) -> Scope<'a> {
        Scope { vars, parent: None }
    }
}

/// What a path lookup resolves to: a value by reference, an array's `length`
/// (a number with no home to borrow), or nothing. Arrays are objects in
/// JavaScript, so `length` and numeric indices are own properties of them.
enum Lookup<'a> {
    Val(&'a Val),
    Num(i64),
    Missing,
}

fn lookup<'a>(scope: &'a Scope<'a>, path: &[String]) -> Lookup<'a> {
    let mut s: Option<&Scope> = Some(scope);
    let mut first: Option<&Val> = None;
    while let Some(sc) = s {
        if let Some(found) = sc.vars.iter().rev().find(|(k, _)| k == &path[0]) {
            first = Some(&found.1);
            break;
        }
        s = sc.parent;
    }
    let mut cur = match first {
        Some(v) => Lookup::Val(v),
        None => return Lookup::Missing,
    };
    for name in &path[1..] {
        cur = match cur {
            Lookup::Val(Val::Obj(fields)) => match Val::obj_get(fields, name) {
                Some(v) => Lookup::Val(v),
                None => Lookup::Missing,
            },
            Lookup::Val(Val::Arr(items)) => {
                if name == "length" {
                    Lookup::Num(items.len() as i64)
                } else {
                    match parse_index(name).and_then(|i| items.get(i)) {
                        Some(v) => Lookup::Val(v),
                        None => Lookup::Missing,
                    }
                }
            }
            _ => Lookup::Missing,
        };
        if matches!(cur, Lookup::Missing) {
            break;
        }
    }
    cur
}

/// Array index access needs an owned number for `length`; resolve a path to
/// an owned value instead of a reference.
fn parse_index(name: &str) -> Option<usize> {
    if name.is_empty() || !name.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if name.len() > 1 && name.starts_with('0') {
        return None;
    }
    name.parse().ok()
}

/// A value as text: a string itself, a number in JavaScript's digits, true or
/// false, an array's items joined by ", ", else "".
fn text_of(v: &Val) -> String {
    match v {
        Val::Str(s) => s.clone(),
        Val::Num(n) => js_num(*n),
        Val::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Val::Arr(items) => {
            let parts: Vec<String> = items.iter().map(text_of).collect();
            parts.join(", ")
        }
        _ => String::new(),
    }
}

/// A lookup as text.
fn text_of_lookup(l: &Lookup) -> String {
    match l {
        Lookup::Val(v) => text_of(v),
        Lookup::Num(n) => js_num(*n),
        Lookup::Missing => String::new(),
    }
}

/// Whether a lookup counts as true in an if.
fn truthy_lookup(l: &Lookup) -> bool {
    match l {
        Lookup::Val(v) => truthy(v),
        Lookup::Num(n) => *n != 0,
        Lookup::Missing => false,
    }
}

/// Whether a value counts as true in an if.
fn truthy(v: &Val) -> bool {
    match v {
        Val::Arr(items) => !items.is_empty(),
        Val::Null => false,
        Val::Bool(b) => *b,
        Val::Str(s) => !s.is_empty(),
        Val::Num(n) => *n != 0,
        Val::Obj(_) => true,
    }
}

/// Rendering context: the parsed templates and the include depth.
pub struct Ctx {
    pub templates: BTreeMap<String, Vec<Node>>,
}

/// `nodes` rendered in `scope`, appended to `out`.
pub fn render_nodes(nodes: &[Node], scope: &Scope, ctx: &Ctx, depth: usize, out: &mut String) {
    for n in nodes {
        match n {
            Node::Text(t) => out.push_str(t),
            Node::Var(p) => {
                let v = lookup(scope, p);
                push_escaped(out, &text_of_lookup(&v));
            }
            Node::Raw(p) => {
                let v = lookup(scope, p);
                out.push_str(&text_of_lookup(&v));
            }
            Node::If { not, path, then, els } => {
                let v = truthy_lookup(&lookup(scope, path));
                let branch = if v != *not { Some(then) } else { els.as_ref() };
                if let Some(b) = branch {
                    render_nodes(b, scope, ctx, depth, out);
                }
            }
            Node::For { name, path, body } => {
                if let Lookup::Val(Val::Arr(items)) = lookup(scope, path) {
                    let len = items.len();
                    for (k, x) in items.iter().enumerate() {
                        let loop_val = Val::Obj(vec![
                            ("index".to_string(), Val::Num(k as i64 + 1)),
                            ("first".to_string(), Val::Bool(k == 0)),
                            ("last".to_string(), Val::Bool(k + 1 == len)),
                            ("length".to_string(), Val::Num(len as i64)),
                        ]);
                        // The element is bound first and `loop` after it, so
                        // a for named `loop` reads the loop object, as the
                        // JavaScript object literal does.
                        let child = Scope {
                            vars: vec![(name.clone(), x.clone()), ("loop".to_string(), loop_val)],
                            parent: Some(scope),
                        };
                        render_nodes(body, &child, ctx, depth, out);
                    }
                }
            }
            Node::Include(name) => {
                if depth < 8 {
                    if let Some(t) = ctx.templates.get(name) {
                        render_nodes(t, scope, ctx, depth + 1, out);
                    }
                }
            }
        }
    }
}
