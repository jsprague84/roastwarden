//! Qt's escaped INI values: quoted parts, `\` escapes, commas making a list, and a
//! `@Variant(…)` (a `QDataStream`) read as Qt writes it.

/// Qt's escaped INI value → its strings: quoted parts, `\` escapes (`\n`, `\xHH…`, octal),
/// commas making a list, and `@Variant(…)` (a `QDataStream`, as Qt writes a one-element list).
pub fn qt_values(raw: &str) -> Vec<String> {
    let chars: Vec<char> = raw.chars().collect();
    let mut items: Vec<String> = Vec::new();
    let mut cur = String::new();
    // Where the unquoted tail of the current item starts, to trim its trailing spaces.
    let mut quoted_upto = 0usize;
    let mut i = 0;
    let mut in_quotes = false;
    let mut list = false;
    let push = |items: &mut Vec<String>, cur: &mut String, quoted_upto: usize| {
        let keep = cur[..quoted_upto].to_string();
        let tail = cur[quoted_upto..].trim_end().to_string();
        items.push(keep + &tail);
        cur.clear();
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            i += 1;
            let Some(&e) = chars.get(i) else { break };
            i += 1;
            let simple = match e {
                'a' => Some('\u{7}'),
                'b' => Some('\u{8}'),
                'f' => Some('\u{c}'),
                'n' => Some('\n'),
                'r' => Some('\r'),
                't' => Some('\t'),
                'v' => Some('\u{b}'),
                '"' | '?' | '\'' | '\\' => Some(e),
                _ => None,
            };
            if let Some(s) = simple {
                cur.push(s);
            } else if e == 'x' {
                let mut v: u32 = 0;
                while let Some(d) = chars.get(i).and_then(|c| c.to_digit(16)) {
                    v = v.saturating_mul(16).saturating_add(d);
                    i += 1;
                }
                cur.push(char::from_u32(v.min(0x0010_FFFF)).unwrap_or('\u{FFFD}'));
            } else if let Some(d) = e.to_digit(8) {
                let mut v = d;
                while let Some(d) = chars.get(i).and_then(|c| c.to_digit(8)) {
                    v = v.saturating_mul(8).saturating_add(d);
                    i += 1;
                }
                cur.push(char::from_u32(v).unwrap_or('\u{FFFD}'));
            }
            if in_quotes {
                quoted_upto = cur.len();
            }
            continue;
        }
        if c == '"' {
            in_quotes = !in_quotes;
            i += 1;
            if !in_quotes {
                quoted_upto = cur.len();
            }
            continue;
        }
        if in_quotes {
            cur.push(c);
            quoted_upto = cur.len();
            i += 1;
            continue;
        }
        if c == ',' {
            list = true;
            push(&mut items, &mut cur, quoted_upto);
            quoted_upto = 0;
            i += 1;
            while chars.get(i).is_some_and(|c| *c == ' ' || *c == '\t') {
                i += 1;
            }
            continue;
        }
        if c == ';' {
            break; // the rest of the line is a comment (Qt quotes a value holding one)
        }
        if cur.is_empty() && (c == ' ' || c == '\t') {
            i += 1;
            continue;
        }
        cur.push(c);
        i += 1;
    }
    if list || !cur.is_empty() || quoted_upto > 0 {
        push(&mut items, &mut cur, quoted_upto);
    }
    // `@Variant(…)` elements; a single one holding a list is that list.
    let mut out = Vec::new();
    let single = items.len() == 1;
    for item in items {
        if let Some(rest) = item.strip_prefix("@@") {
            out.push(format!("@{rest}"));
        } else if item == "@Invalid()" {
            if !single {
                out.push(String::new());
            }
        } else if let Some(inner) = item
            .strip_prefix("@Variant(")
            .and_then(|s| s.strip_suffix(')'))
        {
            let bytes: Vec<u8> = inner.chars().map(|c| c as u32 as u8).collect();
            match qvariant(&bytes) {
                Some(QVar::List(l)) if single => out.extend(l),
                Some(QVar::List(l)) => out.push(l.join(", ")),
                Some(QVar::One(v)) => out.push(v),
                None => out.push(String::new()),
            }
        } else {
            out.push(item);
        }
    }
    out
}

enum QVar {
    One(String),
    List(Vec<String>),
}

/// A `QVariant` from a Qt 4.0 `QDataStream` (what `QSettings` writes): the scalar types and
/// lists of them, as text. Anything else is `None`.
fn qvariant(bytes: &[u8]) -> Option<QVar> {
    struct R<'a>(&'a [u8], usize);
    impl R<'_> {
        fn take(&mut self, n: usize) -> Option<&[u8]> {
            let s = self.0.get(self.1..self.1 + n)?;
            self.1 += n;
            Some(s)
        }
        fn u32(&mut self) -> Option<u32> {
            Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
        }
    }
    fn one(r: &mut R) -> Option<QVar> {
        let ty = r.u32()?;
        Some(match ty {
            1 => QVar::One((r.take(1)?[0] != 0).to_string()),
            2 => QVar::One((r.u32()? as i32).to_string()),
            3 => QVar::One(r.u32()?.to_string()),
            4 => QVar::One(i64::from_be_bytes(r.take(8)?.try_into().ok()?).to_string()),
            5 => QVar::One(u64::from_be_bytes(r.take(8)?.try_into().ok()?).to_string()),
            6 => QVar::One(f64::from_be_bytes(r.take(8)?.try_into().ok()?).to_string()),
            10 => {
                let n = r.u32()?;
                if n == u32::MAX {
                    return Some(QVar::One(String::new()));
                }
                let units: Vec<u16> = r
                    .take(n as usize)?
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| u16::from_be_bytes(*p))
                    .collect();
                QVar::One(String::from_utf16_lossy(&units))
            }
            9 => {
                let n = r.u32()?;
                let mut l = Vec::new();
                for _ in 0..n.min(10_000) {
                    match one(r)? {
                        QVar::One(v) => l.push(v),
                        QVar::List(v) => l.push(v.join(", ")),
                    }
                }
                QVar::List(l)
            }
            _ => return None,
        })
    }
    one(&mut R(bytes, 0))
}
