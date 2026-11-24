// makefun-site's front matter (lib/frontmatter.js): a strict subset of YAML's
// block mappings. Ported faithfully, including every refusal reason and the
// order the checks run in.

use crate::util::js_trim;

const KEYS: [&str; 7] = ["title", "date", "tags", "draft", "layout", "slug", "summary"];

/// One source's parsed front matter. `Option` marks whether the key was given:
/// a post needs a date, and `layout`/`slug` fall back when absent.
#[derive(Default, Debug, Clone)]
pub struct FmData {
    pub title: String,
    pub date: Option<String>,
    pub tags: Vec<String>,
    pub draft: bool,
    pub layout: Option<String>,
    pub slug: Option<String>,
    pub summary: Option<String>,
}

fn days_in(y: i64, m: i64) -> i64 {
    if m == 2 {
        if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
            29
        } else {
            28
        }
    } else if [4, 6, 9, 11].contains(&m) {
        30
    } else {
        31
    }
}

/// A string value: "..." with \" and \\ its only escapes, or bare, the rest
/// of the line (never opening on " or [).
fn string_of(raw: &str) -> Result<String, String> {
    if raw.starts_with('"') {
        if raw.len() < 2 || !raw.ends_with('"') {
            return Err("a quoted string is not closed".to_string());
        }
        let body = &raw[1..raw.len() - 1];
        let mut out = String::with_capacity(body.len());
        let mut chars = body.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some(n @ ('"' | '\\')) => out.push(n),
                    _ => {
                        return Err(
                            "a quoted string holds an escape other than \\\" or \\\\"
                                .to_string(),
                        )
                    }
                }
            } else if c == '"' {
                return Err("a quoted string holds a stray quote".to_string());
            } else {
                out.push(c);
            }
        }
        Ok(out)
    } else if raw.starts_with('[') {
        Err("a bare string opens on [".to_string())
    } else {
        Ok(raw.to_string())
    }
}

/// One key's value, or why it is refused.
fn value_of(key: &str, raw: &str, data: &mut FmData) -> Result<(), String> {
    match key {
        "title" | "summary" => {
            let s = string_of(raw).map_err(|e| format!("key \"{key}\": {e}"))?;
            if key == "title" {
                data.title = s;
            } else {
                data.summary = Some(s);
            }
            Ok(())
        }
        "date" => {
            let b = raw.as_bytes();
            let ok_date = raw.len() == 10
                && b[4] == b'-'
                && b[7] == b'-'
                && b.iter().enumerate().all(|(i, &c)| {
                    i == 4 || i == 7 || c.is_ascii_digit()
                });
            let (y, mo, d) = if ok_date {
                (
                    raw[0..4].parse::<i64>().unwrap_or(0),
                    raw[5..7].parse::<i64>().unwrap_or(0),
                    raw[8..10].parse::<i64>().unwrap_or(0),
                )
            } else {
                (0, 0, 0)
            };
            if !ok_date
                || y < 1970
                || y > 2099
                || !(1..=12).contains(&mo)
                || d < 1
                || d > days_in(y, mo)
            {
                return Err(
                    "key \"date\": not a date from 1970-01-01 to 2099-12-31 (YYYY-MM-DD)"
                        .to_string(),
                );
            }
            data.date = Some(raw.to_string());
            Ok(())
        }
        "tags" => {
            if !raw.starts_with('[') || !raw.ends_with(']') {
                return Err("key \"tags\": not a list [a, b]".to_string());
            }
            let inner = &raw[1..raw.len() - 1];
            if js_trim(inner).is_empty() {
                data.tags = Vec::new();
                return Ok(());
            }
            let mut tags = Vec::new();
            for t in inner.split(',') {
                let t = js_trim(t);
                if t.is_empty() || t.contains('"') || t.contains('[') || t.contains(']') {
                    return Err(
                        "key \"tags\": an empty tag, or one holding \" [ or ]".to_string()
                    );
                }
                tags.push(t.to_string());
            }
            data.tags = tags;
            Ok(())
        }
        "draft" => {
            if raw == "true" {
                data.draft = true;
                Ok(())
            } else if raw == "false" {
                data.draft = false;
                Ok(())
            } else {
                Err("key \"draft\": true or false".to_string())
            }
        }
        "layout" => {
            if !raw.is_empty() && raw.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-') {
                data.layout = Some(raw.to_string());
                Ok(())
            } else {
                Err("key \"layout\": not a template name".to_string())
            }
        }
        "slug" => {
            let ok = raw.len() <= 64
                && !raw.is_empty()
                && raw
                    .split('-')
                    .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
            if ok {
                data.slug = Some(raw.to_string());
                Ok(())
            } else {
                Err("key \"slug\": not a slug".to_string())
            }
        }
        _ => Err(format!("unknown key \"{key}\"")),
    }
}

/// A source's front matter and body: (data, body), or the first reason it is
/// refused. `text` has "\n" line ends.
pub fn parse_front_matter(text: &str) -> Result<(FmData, String), String> {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.is_empty() || lines[0] != "---" {
        return Err("no front matter".to_string());
    }
    let mut close: Option<usize> = None;
    for i in 1..lines.len() {
        if lines[i] == "---" {
            close = Some(i);
            break;
        }
    }
    let close = match close {
        Some(c) => c,
        None => return Err("the front matter is not closed".to_string()),
    };
    let mut data = FmData::default();
    let mut seen: Vec<&str> = Vec::new();
    for i in 1..close {
        let line = lines[i];
        if line.bytes().all(|c| c == b' ' || c == b'\t') || line.starts_with('#') {
            continue;
        }
        // LINE = /^([a-z_]+):(?: (.*))?$/
        let b = line.as_bytes();
        let mut k = 0;
        while k < b.len() && (b[k].is_ascii_lowercase() || b[k] == b'_') {
            k += 1;
        }
        if k == 0 || k >= b.len() || b[k] != b':' {
            return Err(format!("line {} is not \"key: value\"", i + 1));
        }
        let key = &line[..k];
        let raw_full: &str = if k + 1 == b.len() {
            ""
        } else if b[k + 1] == b' ' {
            &line[k + 2..]
        } else {
            return Err(format!("line {} is not \"key: value\"", i + 1));
        };
        let raw = raw_full.trim_end_matches([' ', '\t']);
        if !KEYS.contains(&key) {
            return Err(format!("unknown key \"{key}\""));
        }
        if seen.contains(&key) {
            return Err(format!("key \"{key}\" is given twice"));
        }
        seen.push(key);
        if raw.is_empty() {
            return Err(format!("key \"{key}\" has no value"));
        }
        value_of(key, raw, &mut data)?;
    }
    if !seen.contains(&"title") {
        return Err("no title".to_string());
    }
    Ok((data, lines[close + 1..].join("\n")))
}
