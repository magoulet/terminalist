//! Text editing for dialog fields: key handling on a buffer with a character cursor, and the
//! soft wrapping the multi-line description field needs to draw itself and move up and down.
//!
//! Widths count characters, not terminal cells, so wide glyphs such as CJK or emoji wrap a
//! little late. That matches how the single-line fields already place their cursor.

use crossterm::event::KeyCode;

/// Byte offset of a character cursor in `buffer`.
fn byte_pos(buffer: &str, cursor: usize) -> usize {
    buffer.chars().take(cursor).map(char::len_utf8).sum()
}

/// Applies an editing key to `buffer`. Returns whether the text changed. Home and End go to
/// the start and end of the current line, which is the whole text in a single-line field.
pub fn edit(buffer: &mut String, cursor: &mut usize, key: KeyCode) -> bool {
    let len = buffer.chars().count();
    *cursor = (*cursor).min(len);
    match key {
        KeyCode::Char(c) => {
            buffer.insert(byte_pos(buffer, *cursor), c);
            *cursor += 1;
            true
        }
        KeyCode::Backspace if *cursor > 0 => {
            let at = byte_pos(buffer, *cursor - 1);
            buffer.remove(at);
            *cursor -= 1;
            true
        }
        KeyCode::Delete if *cursor < len => {
            buffer.remove(byte_pos(buffer, *cursor));
            true
        }
        KeyCode::Left => {
            *cursor = cursor.saturating_sub(1);
            false
        }
        KeyCode::Right => {
            *cursor = (*cursor + 1).min(len);
            false
        }
        KeyCode::Home => {
            *cursor = line_start(buffer, *cursor);
            false
        }
        KeyCode::End => {
            *cursor = line_end(buffer, *cursor);
            false
        }
        _ => false,
    }
}

/// Inserts a line break at the cursor.
pub fn insert_newline(buffer: &mut String, cursor: &mut usize) {
    edit(buffer, cursor, KeyCode::Char('\n'));
}

fn line_start(buffer: &str, cursor: usize) -> usize {
    let chars: Vec<char> = buffer.chars().collect();
    (0..cursor.min(chars.len()))
        .rev()
        .find(|&i| chars[i] == '\n')
        .map_or(0, |i| i + 1)
}

fn line_end(buffer: &str, cursor: usize) -> usize {
    let chars: Vec<char> = buffer.chars().collect();
    (cursor..chars.len()).find(|&i| chars[i] == '\n').unwrap_or(chars.len())
}

/// One drawn row of wrapped text, as a character range of the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub start: usize,
    pub len: usize,
    /// The next row continues this line (a soft wrap) rather than following a line break.
    pub wraps: bool,
}

/// Splits `text` into rows of at most `width` characters. Every line break starts a new row
/// and an empty line still takes one.
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<Row> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut start = 0;
    for line in text.split('\n') {
        let len = line.chars().count();
        let mut offset = 0;
        loop {
            let take = (len - offset).min(width);
            let wraps = offset + take < len;
            rows.push(Row {
                start: start + offset,
                len: take,
                wraps,
            });
            offset += take;
            if !wraps {
                break;
            }
        }
        start += len + 1; // the line break
    }
    rows
}

/// Row and column of the cursor in the wrapped text. A cursor right at a soft wrap sits at
/// the start of the next row, where the next character typed would appear.
#[must_use]
pub fn cursor_cell(rows: &[Row], cursor: usize) -> (usize, usize) {
    for (index, row) in rows.iter().enumerate() {
        let end = row.start + row.len;
        if cursor >= row.start && (cursor < end || (cursor == end && !row.wraps)) {
            return (index, cursor - row.start);
        }
    }
    let last = rows.len().saturating_sub(1);
    (last, rows.get(last).map_or(0, |row| row.len))
}

/// Moves the cursor one wrapped row up or down, keeping its column where the row allows.
/// Returns whether it moved.
pub fn move_vertical(buffer: &str, cursor: &mut usize, width: usize, down: bool) -> bool {
    let rows = wrap(buffer, width);
    let (row, column) = cursor_cell(&rows, *cursor);
    let target = if down { row + 1 } else { row.wrapping_sub(1) };
    let Some(next) = rows.get(target) else {
        return false;
    };
    // On a soft-wrapped row the last position belongs to the row below.
    let last = if next.wraps {
        next.len.saturating_sub(1)
    } else {
        next.len
    };
    *cursor = next.start + column.min(last);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> (String, usize) {
        let mut buffer = String::new();
        let mut cursor = 0;
        for c in text.chars() {
            if c == '\n' {
                insert_newline(&mut buffer, &mut cursor);
            } else {
                edit(&mut buffer, &mut cursor, KeyCode::Char(c));
            }
        }
        (buffer, cursor)
    }

    #[test]
    fn editing_handles_multibyte_characters() {
        let (mut buffer, mut cursor) = typed("café");
        assert!(edit(&mut buffer, &mut cursor, KeyCode::Backspace));
        assert_eq!(buffer, "caf");
        edit(&mut buffer, &mut cursor, KeyCode::Left);
        assert!(edit(&mut buffer, &mut cursor, KeyCode::Char('é')));
        assert_eq!(buffer, "caéf");
    }

    #[test]
    fn home_and_end_stay_on_the_current_line() {
        let (buffer, mut cursor) = typed("first\nsecond");
        edit(&mut buffer.clone(), &mut cursor, KeyCode::Home);
        assert_eq!(cursor, 6);
        cursor = 2;
        edit(&mut buffer.clone(), &mut cursor, KeyCode::End);
        assert_eq!(cursor, 5);
    }

    #[test]
    fn wrap_splits_long_lines_and_keeps_empty_ones() {
        let rows = wrap("abcdef\n\nxy", 4);
        let ranges: Vec<(usize, usize, bool)> = rows.iter().map(|r| (r.start, r.len, r.wraps)).collect();
        assert_eq!(ranges, vec![(0, 4, true), (4, 2, false), (7, 0, false), (8, 2, false)]);
    }

    #[test]
    fn cursor_at_a_soft_wrap_shows_on_the_next_row() {
        let rows = wrap("abcdef", 4);
        assert_eq!(cursor_cell(&rows, 3), (0, 3));
        assert_eq!(cursor_cell(&rows, 4), (1, 0));
        assert_eq!(cursor_cell(&rows, 6), (1, 2));
    }

    #[test]
    fn cursor_at_a_line_break_stays_on_its_line() {
        let rows = wrap("ab\ncd", 10);
        assert_eq!(cursor_cell(&rows, 2), (0, 2));
        assert_eq!(cursor_cell(&rows, 3), (1, 0));
    }

    #[test]
    fn vertical_moves_keep_the_column_and_clamp_to_short_rows() {
        let text = "long line\nab\nlonger line";
        let mut cursor = 7; // "long li|ne"
        assert!(move_vertical(text, &mut cursor, 20, true));
        assert_eq!(cursor, 12); // end of "ab"
        assert!(move_vertical(text, &mut cursor, 20, true));
        assert_eq!(cursor, 15); // column 2 of "longer line"
        assert!(!move_vertical(text, &mut cursor, 20, true));
        assert!(move_vertical(text, &mut cursor, 20, false));
        assert_eq!(cursor, 12);
    }
}
