#[derive(Debug, Clone, Default)]
pub struct TextInput {
    pub buffer: String,
    pub cursor: usize,
    pub goal_column: Option<usize>,
}

impl TextInput {
    pub fn new(text: String) -> Self {
        let cursor = text.chars().count();
        Self {
            buffer: text,
            cursor,
            goal_column: None,
        }
    }

    pub fn byte_index(&self) -> usize {
        self.buffer
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.buffer.len())
    }

    pub fn insert(&mut self, c: char) {
        let i = self.byte_index();
        self.buffer.insert(i, c);
        self.cursor += 1;
        self.goal_column = None;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            let i = self.byte_index();
            self.buffer.remove(i);
        }
        self.goal_column = None;
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
        self.goal_column = None;
    }

    pub fn delete_word(&mut self) {
        let chars: Vec<char> = self.buffer.chars().collect();
        let end = self.cursor.min(chars.len());
        let trimmed = chars[..end]
            .iter()
            .rposition(|c| !c.is_whitespace())
            .map_or(0, |i| i + 1);
        if trimmed == 0 {
            return;
        }
        let start = chars[..trimmed]
            .iter()
            .rposition(|c| c.is_whitespace())
            .map_or(0, |i| i + 1);

        let byte_start = char_to_byte(&self.buffer, start);
        let byte_end = char_to_byte(&self.buffer, end);
        self.buffer.replace_range(byte_start..byte_end, "");
        self.cursor = start;
        self.goal_column = None;
    }

    pub fn move_down(&mut self) {
        let chars: Vec<char> = self.buffer.chars().collect();
        let start = line_start(&chars, self.cursor);
        let end = line_end(&chars, self.cursor);
        let column = *self.goal_column.get_or_insert(self.cursor - start);
        if end == chars.len() {
            return;
        }

        let next_start = end + 1;
        let next_end = line_end(&chars, next_start);
        self.cursor = (next_start + column).min(next_end);
    }

    pub fn move_up(&mut self) {
        let chars: Vec<char> = self.buffer.chars().collect();
        let start = line_start(&chars, self.cursor);
        let column = *self.goal_column.get_or_insert(self.cursor - start);
        if start == 0 {
            return;
        }

        let prev_end = start - 1;
        let prev_start = line_start(&chars, prev_end);
        self.cursor = (prev_start + column).min(prev_end);
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
        self.goal_column = None;
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.buffer.chars().count());
        self.goal_column = None;
    }
}

fn char_to_byte(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

fn line_start(chars: &[char], pos: usize) -> usize {
    chars[..pos]
        .iter()
        .rposition(|&c| c == '\n')
        .map_or(0, |newline| newline + 1)
}

fn line_end(chars: &[char], pos: usize) -> usize {
    chars[pos..]
        .iter()
        .position(|&c| c == '\n')
        .map_or(chars.len(), |offset| pos + offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(text: &str, cursor: usize) -> TextInput {
        TextInput {
            buffer: text.into(),
            cursor,
            goal_column: None,
        }
    }

    #[test]
    fn insert_lands_on_char_boundaries() {
        let mut e = input("aple", 0);
        e.insert('m');
        assert_eq!((e.buffer.as_str(), e.cursor), ("maple", 1));

        let mut e = input("mple", 1);
        e.insert('a');
        assert_eq!((e.buffer.as_str(), e.cursor), ("maple", 2));

        let mut e = input("mapl", 4);
        e.insert('e');
        assert_eq!((e.buffer.as_str(), e.cursor), ("maple", 5));

        let mut e = input("é1", 1);
        e.insert('x');
        assert_eq!((e.buffer.as_str(), e.cursor), ("éx1", 2));
    }

    #[test]
    fn backspace_removes_a_whole_multibyte_character() {
        let mut e = input("maplé", 5);
        e.backspace();
        assert_eq!(e.buffer, "mapl");
        assert_eq!(e.cursor, 4);
    }

    #[test]
    fn cursor_clamps_at_both_ends() {
        let mut e = input("maple", 0);
        e.move_left();
        e.backspace();
        assert_eq!((e.buffer.as_str(), e.cursor), ("maple", 0));

        let mut e = input("maple", 5);
        e.move_right();
        assert_eq!(e.cursor, 5);
    }

    #[test]
    fn vertical_movement_remembers_the_goal_column() {
        let mut e = input("mapleberry\nmap\nmapleberry", 8);
        e.move_down();
        assert_eq!(e.cursor, 14);
        e.move_down();
        assert_eq!(e.cursor, 23);

        e.move_up();
        assert_eq!(e.cursor, 14);
        e.move_up();
        assert_eq!(e.cursor, 8);

        e.move_down();
        e.insert('x');
        e.move_down();
        assert_eq!(e.cursor, 20);
    }

    #[test]
    fn new_puts_the_cursor_at_the_end() {
        let e = TextInput::new("maplé".into());
        assert_eq!(e.cursor, 5, "cursor counts chars, not bytes");
    }

    #[test]
    fn delete_word_takes_the_whitespace_with_it() {
        let mut e = input("foo bar  baz ", 13);
        e.delete_word();
        assert_eq!((e.buffer.as_str(), e.cursor), ("foo bar  ", 9));
        e.delete_word();
        assert_eq!((e.buffer.as_str(), e.cursor), ("foo ", 4));

        // char indices, not byte indices
        let mut e = input("héllo wörld", 11);
        e.delete_word();
        assert_eq!((e.buffer.as_str(), e.cursor), ("héllo ", 6));

        // whitespace on its own is a no-op, like glab-tui's pop_search_word
        let mut e = input("   ", 3);
        e.delete_word();
        assert_eq!((e.buffer.as_str(), e.cursor), ("   ", 3));

        let mut e = input("", 0);
        e.delete_word();
        assert_eq!((e.buffer.as_str(), e.cursor), ("", 0));

        // only what is behind the cursor goes
        let mut e = input("foo bar", 4);
        e.delete_word();
        assert_eq!((e.buffer.as_str(), e.cursor), ("bar", 0));
    }

    #[test]
    fn clear_empties_the_buffer_and_the_cursor() {
        let mut e = input("maple", 3);
        e.goal_column = Some(2);
        e.clear();
        assert_eq!((e.buffer.as_str(), e.cursor), ("", 0));
        assert_eq!(e.goal_column, None);
    }
}
