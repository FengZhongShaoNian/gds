//! Case-insensitive natural filename sorting for sibling image navigation.

#[cfg(test)]
mod tests;

use std::{cmp::Ordering, path::Path};

/// Compares two strings naturally (`IMG_2.png < IMG_10.png`) with deterministic tie-breaking.
pub fn compare_natural(a: &str, b: &str) -> Ordering {
    let primary = compare_chunks(a, b);
    if primary != Ordering::Equal {
        return primary;
    }
    a.cmp(b)
}

/// Compares two filesystem paths by their natural file names, falling back to full paths.
pub fn compare_paths(a: &Path, b: &Path) -> Ordering {
    let name_a = a.file_name().unwrap_or_default();
    let name_b = b.file_name().unwrap_or_default();
    let by_name = match (name_a.to_str(), name_b.to_str()) {
        (Some(sa), Some(sb)) => compare_natural(sa, sb),
        // Non-UTF-8 file names are ordered deterministically after valid UTF-8 without allocation.
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => name_a.as_encoded_bytes().cmp(name_b.as_encoded_bytes()),
    };
    if by_name != Ordering::Equal {
        by_name
    } else {
        a.as_os_str()
            .as_encoded_bytes()
            .cmp(b.as_os_str().as_encoded_bytes())
    }
}

fn compare_chunks(a: &str, b: &str) -> Ordering {
    let mut sa = LoweredStream::new(a);
    let mut sb = LoweredStream::new(b);

    loop {
        sa.flush_empty_pending();
        sb.flush_empty_pending();

        if sa.pending.is_none() && sb.pending.is_none() {
            let Some(&ba) = sa.rem.as_bytes().first() else {
                return if sb.rem.is_empty() {
                    Ordering::Equal
                } else {
                    Ordering::Less
                };
            };
            let Some(&bb) = sb.rem.as_bytes().first() else {
                return Ordering::Greater;
            };
            if ba.is_ascii_digit() && bb.is_ascii_digit() {
                let ord = compare_numeric_slices(sa.take_digits(), sb.take_digits());
                if ord != Ordering::Equal {
                    return ord;
                }
                continue;
            }
            if ba < 128 && bb < 128 {
                let ord = ba.to_ascii_lowercase().cmp(&bb.to_ascii_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                sa.rem = &sa.rem[1..];
                sb.rem = &sb.rem[1..];
                continue;
            }
        }

        match (sa.next_lowered(), sb.next_lowered()) {
            (Some(ca), Some(cb)) => {
                let ord = ca.cmp(&cb);
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
        }
    }
}

struct LoweredStream<'a> {
    rem: &'a str,
    pending: Option<std::iter::Peekable<std::char::ToLowercase>>,
}

impl<'a> LoweredStream<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            rem: s,
            pending: None,
        }
    }

    fn flush_empty_pending(&mut self) {
        if let Some(pending) = &mut self.pending
            && pending.peek().is_none()
        {
            self.pending = None;
        }
    }

    fn take_digits(&mut self) -> &'a str {
        let len = self.rem.bytes().take_while(u8::is_ascii_digit).count();
        let (digits, rest) = self.rem.split_at(len);
        self.rem = rest;
        digits
    }

    fn next_lowered(&mut self) -> Option<char> {
        self.flush_empty_pending();
        if let Some(pending) = &mut self.pending {
            let next = pending.next();
            self.flush_empty_pending();
            return next;
        }
        let first_byte = *self.rem.as_bytes().first()?;
        if first_byte < 128 {
            self.rem = &self.rem[1..];
            return Some((first_byte as char).to_ascii_lowercase());
        }
        let ch = self.rem.chars().next()?;
        self.rem = &self.rem[ch.len_utf8()..];
        let mut iter = ch.to_lowercase().peekable();
        let first = iter.next();
        if iter.peek().is_some() {
            self.pending = Some(iter);
        }
        first
    }
}

fn compare_numeric_slices(a: &str, b: &str) -> Ordering {
    let trim_a = a.trim_start_matches('0');
    let trim_b = b.trim_start_matches('0');
    match trim_a.len().cmp(&trim_b.len()) {
        Ordering::Equal => match trim_a.cmp(trim_b) {
            Ordering::Equal => a.len().cmp(&b.len()),
            other => other,
        },
        other => other,
    }
}
