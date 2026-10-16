//! UTF-8 editor state. Platform input ranges are UTF-16; conversion happens here.
//! Composition is a disposable preview and never the document's persisted text.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
struct Composition {
    original: String,
    selection: Range<usize>,
    reversed: bool,
    range: Range<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Buffer {
    pub text: String,
    pub selection: Range<usize>,
    pub reversed: bool,
    composition: Option<Composition>,
}

pub fn from_utf16(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        // A broken surrogate range snaps left, never into a UTF-8 sequence.
        if units + ch.len_utf16() > offset {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

pub fn to_utf16(text: &str, offset: usize) -> usize {
    text.char_indices()
        .take_while(|(byte, _)| *byte < offset.min(text.len()))
        .map(|(_, ch)| ch.len_utf16())
        .sum()
}

impl Buffer {
    pub fn new(text: String) -> Self {
        Self {
            text,
            ..Self::default()
        }
    }
    pub fn marked(&self) -> Option<Range<usize>> {
        self.composition.as_ref().map(|c| c.range.clone())
    }
    pub fn composing(&self) -> bool {
        self.composition.is_some()
    }
    pub fn committed(&self) -> &str {
        self.composition
            .as_ref()
            .map_or(&self.text, |c| &c.original)
    }
    pub fn caret(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }
    pub fn move_to(&mut self, byte: usize) {
        let byte = self.snap(byte);
        self.selection = byte..byte;
        self.reversed = false;
    }
    pub fn select_to(&mut self, byte: usize) {
        let byte = self.snap(byte);
        let anchor = if self.reversed {
            self.selection.end
        } else {
            self.selection.start
        };
        self.selection = byte.min(anchor)..byte.max(anchor);
        self.reversed = byte < anchor;
    }
    pub fn snap(&self, mut byte: usize) -> usize {
        byte = byte.min(self.text.len());
        while !self.text.is_char_boundary(byte) {
            byte -= 1;
        }
        byte
    }
    pub fn previous(&self) -> usize {
        self.text
            .grapheme_indices(true)
            .rev()
            .find_map(|(i, _)| (i < self.caret()).then_some(i))
            .unwrap_or(0)
    }
    pub fn next(&self) -> usize {
        self.text
            .grapheme_indices(true)
            .find_map(|(i, _)| (i > self.caret()).then_some(i))
            .unwrap_or(self.text.len())
    }
    pub fn previous_word(&self) -> usize {
        UnicodeSegmentation::unicode_word_indices(self.text.as_str())
            .take_while(|(start, _)| *start < self.caret())
            .map(|(start, _)| start)
            .last()
            .unwrap_or(0)
    }
    pub fn next_word(&self) -> usize {
        let caret = self.caret();
        UnicodeSegmentation::unicode_word_indices(self.text.as_str())
            .find_map(|(start, _)| (start > caret).then_some(start))
            .unwrap_or_else(|| {
                if caret < self.text.len() {
                    self.next()
                } else {
                    self.text.len()
                }
            })
    }
    pub fn utf8_range(&self, range: &Range<usize>) -> Range<usize> {
        let start = from_utf16(&self.text, range.start);
        start..from_utf16(&self.text, range.end).max(start)
    }
    pub fn utf16_range(&self, range: &Range<usize>) -> Range<usize> {
        to_utf16(&self.text, range.start)..to_utf16(&self.text, range.end)
    }
    pub fn replace(&mut self, range_utf16: Option<Range<usize>>, new_text: &str) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.utf8_range(r))
            .or_else(|| self.marked())
            .unwrap_or(self.selection.clone());
        self.text.replace_range(range.clone(), new_text);
        self.composition = None;
        self.move_to(range.start + new_text.len());
    }
    pub fn mark(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        selected: Option<Range<usize>>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.utf8_range(r))
            .or_else(|| self.marked())
            .unwrap_or(self.selection.clone());
        if self.composition.is_none() {
            self.composition = Some(Composition {
                original: self.text.clone(),
                selection: self.selection.clone(),
                reversed: self.reversed,
                range: range.clone(),
            });
        }
        if new_text.is_empty() {
            self.cancel();
            return;
        }
        self.text.replace_range(range.clone(), new_text);
        self.composition.as_mut().unwrap().range = range.start..range.start + new_text.len();
        let selected = selected.unwrap_or_else(|| {
            let end = new_text.encode_utf16().count();
            end..end
        });
        self.selection = range.start + from_utf16(new_text, selected.start)
            ..range.start + from_utf16(new_text, selected.end);
        self.reversed = false;
    }
    pub fn unmark(&mut self) {
        self.composition = None;
    }
    pub fn cancel(&mut self) {
        if let Some(c) = self.composition.take() {
            self.text = c.original;
            self.selection = c.selection;
            self.reversed = c.reversed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composition_relative_utf16_range_uses_inserted_text_and_start_for_both_ends() {
        let mut b = Buffer::new("前🦀后".into());
        b.selection = "前".len().."前🦀".len();
        b.mark(None, "中文🦀", Some(1..4));
        assert_eq!(b.text, "前中文🦀后");
        assert_eq!(b.selection, 6..13);
        assert_eq!(b.committed(), "前🦀后");
        b.replace(None, "模型");
        assert_eq!(b.committed(), "前模型后");
        assert_eq!(b.caret(), "前模型".len());
    }
    #[test]
    fn canceled_composition_restores_replaced_selection_and_does_not_leak_preview() {
        let mut b = Buffer::new("选择这里".into());
        b.selection = 0..6;
        b.mark(None, "xuan", None);
        b.mark(None, "选择", Some(2..2));
        b.mark(None, "", None);
        assert_eq!(b.text, "选择这里");
        assert_eq!(b.selection, 0..6);
        assert!(!b.composing());
    }
    #[test]
    fn surrogate_boundaries_and_graphemes_remain_valid() {
        let mut b = Buffer::new("中👩‍💻e\u{301}".into());
        for byte in b.text.char_indices().map(|(i, _)| i).chain([b.text.len()]) {
            assert_eq!(from_utf16(&b.text, to_utf16(&b.text, byte)), byte);
        }
        assert_eq!(from_utf16(&b.text, 2), "中".len());
        b.move_to(b.text.len());
        let previous = b.previous();
        b.selection = previous..b.text.len();
        b.replace(None, "");
        assert_eq!(b.text, "中👩‍💻");
    }

    #[test]
    fn word_navigation_skips_whitespace_and_stays_on_utf8_boundaries() {
        let text = "hello, world 中文";
        let mut b = Buffer::new(text.to_owned());
        b.move_to(0);
        assert_eq!(b.next_word(), "hello, ".len());

        b.move_to("hel".len());
        assert_eq!(b.next_word(), "hello, ".len());

        b.move_to(text.len());
        let before = b.previous_word();
        assert!(before < text.len());
        assert!(text.is_char_boundary(before));

        b.move_to("hello, ".len());
        let after = b.next_word();
        assert_eq!(after, "hello, world ".len());
        assert!(text.is_char_boundary(after));
        assert_eq!(b.previous_word(), 0);

        b.move_to("hello, world".len());
        assert!(text.is_char_boundary(b.next_word()));
    }
}
