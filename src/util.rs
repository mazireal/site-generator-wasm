// Shared helpers that mirror JavaScript's exact semantics, since byte-identical
// output against makefun-site 1.0.0 is the whole ballgame. Written from the
// baseline's README and code as a reference; all code here is original.

use std::cmp::Ordering;
use unicode_categories::UnicodeCategories;
use unicode_normalization::UnicodeNormalization;

/// Compare two strings by UTF-16 code unit, the order JavaScript's `<` uses
/// for strings (and that makefun-site sorts by). Rust's `str` ordering is by
/// bytes (code points), which differs from UTF-16 order for characters above
/// the BMP versus U+E000..U+FFFF.
pub fn cmp_u16(a: &str, b: &str) -> Ordering {
    // ASCII bytes order exactly like their UTF-16 code units.
    if a.is_ascii() && b.is_ascii() {
        return a.cmp(b);
    }
    a.encode_utf16().cmp(b.encode_utf16())
}

/// JavaScript's `\s`: WhiteSpace plus LineTerminator. Note this differs from
/// Rust's `char::is_whitespace` in two places: JS includes U+FEFF, Rust
/// includes U+0085 (NEL) which JS does not.
pub fn is_js_ws(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | ' '
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

/// JavaScript's `String.prototype.trim`: WhiteSpace + LineTerminator at both ends.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_ws)
}

/// Whether `c` is one of the characters a backslash escapes in makefun-site's
/// Markdown (markdown.js ESCAPABLE).
pub fn is_escapable(c: char) -> bool {
    matches!(
        c,
        '\\' | '`' | '*' | '_' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.' | '!' | '>'
    )
}

/// Append `text` to `out` with & < > " ' escaped (text.js escapeHtml).
/// Maximal runs of bytes needing no escape are copied at once (the escapable
/// bytes are all ASCII, so a run never splits a multi-byte character).
pub fn push_escaped(out: &mut String, text: &str) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let mut j = i;
        while j < b.len() {
            match b[j] {
                b'&' | b'<' | b'>' | b'"' | b'\'' => break,
                _ => j += 1,
            }
        }
        if j > i {
            out.push_str(&text[i..j]);
        }
        if j < b.len() {
            out.push_str(match b[j] {
                b'&' => "&amp;",
                b'<' => "&lt;",
                b'>' => "&gt;",
                b'"' => "&quot;",
                _ => "&#39;",
            });
            j += 1;
        }
        i = j;
    }
}

/// The HTML escape table applied to one character (for the inline renderer).
pub fn escape_char(c: char) -> &'static str {
    match c {
        '&' => "&amp;",
        '<' => "&lt;",
        '>' => "&gt;",
        '"' => "&quot;",
        '\'' => "&#39;",
        _ => "",
    }
}

/// The slug of `text` (text.js slugify): NFKD, combining marks dropped, A-Z
/// lowered, each run of characters other than a-z and 0-9 one `-`, none at
/// either end, at most 64 characters (a `-` left at the cut dropped), and
/// "untitled" when nothing is left.
pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = false;
    // The step a character takes through the slug rule.
    let step = |ch: char, out: &mut String, gap: &mut bool| {
        let keep = if ch.is_ascii_uppercase() {
            Some(ch.to_ascii_lowercase())
        } else if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            Some(ch)
        } else {
            None
        };
        match keep {
            Some(k) => {
                if *gap {
                    out.push('-');
                }
                *gap = false;
                out.push(k);
            }
            None => {
                *gap = !out.is_empty();
            }
        }
    };
    if text.is_ascii() {
        // NFKD is the identity on ASCII, and no ASCII character is a mark.
        for ch in text.chars() {
            step(ch, &mut out, &mut gap);
        }
    } else {
        for ch in text.nfkd().filter(|c| !c.is_mark()) {
            step(ch, &mut out, &mut gap);
        }
    }
    if out.len() > 64 {
        out.truncate(64);
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "untitled".to_string()
    } else {
        out
    }
}

/// `slug` made unique against `taken` (and added to it): the first of slug,
/// slug-2, slug-3 ... not taken (text.js uniqueSlug).
pub fn unique_slug(slug: &str, taken: &mut std::collections::BTreeSet<String>) -> String {
    let mut out = slug.to_string();
    let mut n = 2;
    while taken.contains(&out) {
        out = format!("{slug}-{n}");
        n += 1;
    }
    taken.insert(out.clone());
    out
}

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September",
    "October", "November", "December",
];

/// A YYYY-MM-DD date in words, from the string alone (text.js longDate).
pub fn long_date(date: &str) -> String {
    let mut parts = date.split('-');
    let y = parts.next().unwrap_or("");
    let m = parts.next().unwrap_or("");
    let d = parts.next().unwrap_or("");
    let day: u32 = d.parse().unwrap_or(0);
    let month: usize = m.parse().unwrap_or(0);
    let name = if (1..=12).contains(&month) {
        MONTHS[month - 1]
    } else {
        ""
    };
    format!("{day} {name} {y}")
}

/// The words of a body: its runs of characters other than (JavaScript)
/// whitespace (index.js wordsOf). ASCII bytes decide directly; only a
/// multi-byte character needs decoding to test for whitespace.
pub fn words_of(text: &str) -> usize {
    let b = text.as_bytes();
    let mut n = 0;
    let mut in_word = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c < 0x80 {
            if matches!(c, 9 | 10 | 11 | 12 | 13 | 32) {
                in_word = false;
            } else if !in_word {
                in_word = true;
                n += 1;
            }
            i += 1;
        } else {
            let ch = text[i..].chars().next().unwrap();
            if is_js_ws(ch) {
                in_word = false;
            } else if !in_word {
                in_word = true;
                n += 1;
            }
            i += ch.len_utf8();
        }
    }
    n
}

/// `rel` (a link's path, "/"-separated) from the directory `dir` of a source,
/// or None when it climbs out of the root (index.js resolvePath).
pub fn resolve_path(dir: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for seg in rel.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            if parts.is_empty() {
                return None;
            }
            parts.pop();
        } else {
            parts.push(seg);
        }
    }
    Some(parts.join("/"))
}

/// The directory of a path: everything before its last `/`, or "" (index.js dirOf).
pub fn dir_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

/// JavaScript's `JSON.stringify` of a string value (used for template parse
/// errors, which name the offending path with it).
pub fn js_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// JavaScript's `String(x)` for a number that is an integer (every number the
/// templates ever print — counts, indices, reading minutes — is one).
pub fn js_num(n: i64) -> String {
    n.to_string()
}

/// JavaScript's `ToIntegerOrInfinity` truncated toward zero, for slice indices.
pub fn js_to_integer(f: f64) -> i64 {
    if f.is_nan() {
        0
    } else if f == f64::INFINITY {
        i64::MAX
    } else if f == f64::NEG_INFINITY {
        i64::MIN
    } else {
        f.trunc() as i64
    }
}

/// Rewrite lone-surrogate `\uXXXX` escapes in raw JSON bytes to `\uFFFD`, so
/// serde_json (which rejects them) accepts input JavaScript's JSON.parse
/// would accept. A high surrogate not immediately followed by a low-surrogate
/// escape, and a low surrogate not immediately preceded by a kept high one,
/// are rewritten in place (same 6-byte length). Valid pairs are left alone.
/// The generator never emits these, but a single one must not fail a run.
pub fn sanitize_lone_surrogates(bytes: &mut [u8]) {
    let n = bytes.len();
    let mut i = 0;
    let mut in_str = false;
    let mut prev_kept_high = false;
    while i < n {
        let b = bytes[i];
        if !in_str {
            if b == b'"' {
                in_str = true;
            }
            i += 1;
            continue;
        }
        if b == b'"' {
            in_str = false;
            prev_kept_high = false;
            i += 1;
            continue;
        }
        if b == b'\\' {
            if i + 1 < n && bytes[i + 1] == b'u' && i + 5 < n {
                let hex = |k: usize| -> Option<u16> {
                    let mut v: u16 = 0;
                    for j in 0..4 {
                        let c = bytes[k + j];
                        let d = match c {
                            b'0'..=b'9' => (c - b'0') as u16,
                            b'a'..=b'f' => (c - b'a' + 10) as u16,
                            b'A'..=b'F' => (c - b'A' + 10) as u16,
                            _ => return None,
                        };
                        v = (v << 4) | d;
                    }
                    Some(v)
                };
                if let Some(cp) = hex(i + 2) {
                    if (0xD800..0xDC00).contains(&cp) {
                        // High surrogate: kept only if a low-surrogate escape follows at once.
                        let mut low_follows = false;
                        if i + 11 < n && bytes[i + 6] == b'\\' && bytes[i + 7] == b'u' {
                            if let Some(lo) = hex(i + 8) {
                                low_follows = (0xDC00..0xE000).contains(&lo);
                            }
                        }
                        if low_follows {
                            prev_kept_high = true;
                        } else {
                            bytes[i + 2..i + 6].copy_from_slice(b"FFFD");
                            prev_kept_high = false;
                        }
                        i += 6;
                        continue;
                    } else if (0xDC00..0xE000).contains(&cp) {
                        if !prev_kept_high {
                            bytes[i + 2..i + 6].copy_from_slice(b"FFFD");
                        }
                        prev_kept_high = false;
                        i += 6;
                        continue;
                    }
                    prev_kept_high = false;
                    i += 6;
                    continue;
                }
                // Not a valid \uXXXX escape (invalid JSON anyway): skip just the backslash.
                prev_kept_high = false;
                i += 2;
                continue;
            }
            // Any other escape: skip the backslash and the escaped character.
            prev_kept_high = false;
            i += 2;
            continue;
        }
        prev_kept_high = false;
        i += 1;
    }
}
