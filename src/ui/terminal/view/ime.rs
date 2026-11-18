//! IME preedit offset helpers; screen output is never exposed as an editable document.

#[cfg(test)]
mod tests;

use std::ops::Range;

#[cfg_attr(not(test), allow(dead_code))] // Seed skeleton for Wave 2 ui-terminal-tools.
fn utf8_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (offset, ch) in text.char_indices() {
        if units + ch.len_utf16() > utf16 {
            return offset;
        }
        units += ch.len_utf16();
    }
    text.len()
}

#[cfg_attr(not(test), allow(dead_code))] // Seed skeleton for Wave 2 ui-terminal-tools.
pub(super) fn byte_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = utf8_offset(text, range.start);
    start..utf8_offset(text, range.end).max(start)
}
