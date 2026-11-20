//! Picker dialogs: a query field over a list of choices, used by the project filter and by
//! "move under" for subtasks.

use super::common::{self, shortcuts};
use crate::theme::Theme;
use crate::ui::layout::LayoutManager;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState},
    Frame,
};

/// One row of a picker.
pub struct PickerRow {
    pub label: String,
    pub depth: usize,
    /// Marks the choice already in effect.
    pub current: bool,
}

/// What a picker dialog shows.
pub struct PickerView<'a> {
    pub title: &'a str,
    pub list_title: &'a str,
    pub action: &'static str,
    pub query: &'a str,
    pub cursor: usize,
    pub rows: Vec<PickerRow>,
    pub selected: usize,
}

pub fn render_picker_dialog(f: &mut Frame, area: Rect, view: &PickerView, theme: &Theme) {
    // Tall enough for a useful list, never taller than the screen.
    let lines = area.height.saturating_sub(4).clamp(10, 24);
    let dialog_area = LayoutManager::centered_rect_lines(50, lines, area);
    f.render_widget(Clear, dialog_area);

    let main_block = common::create_dialog_block(view.title, theme.info);
    let inner_area = main_block.inner(dialog_area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Query field
            Constraint::Min(3),    // Choices
            Constraint::Length(1), // Instructions
        ])
        .split(inner_area);

    let input_paragraph = common::create_input_paragraph(view.query, view.cursor, "Type to narrow", theme);

    let items: Vec<ListItem> = view
        .rows
        .iter()
        .map(|row| {
            let marker = if row.current { "● " } else { "  " };
            let indent = "  ".repeat(row.depth);
            ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(theme.info)),
                Span::styled(format!("{indent}{}", row.label), Style::default().fg(theme.text)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(" {} ", view.list_title))
                .title_style(Style::default().fg(theme.text))
                .border_style(Style::default().fg(theme.border_dim)),
        )
        .highlight_style(Style::default().bg(theme.selection_bg).add_modifier(Modifier::BOLD));
    let mut list_state = ListState::default();
    list_state.select(Some(view.selected.min(view.rows.len().saturating_sub(1))));

    let instructions = [
        ("Enter", theme.success, view.action),
        shortcuts::separator(theme),
        ("↑/↓", theme.info, " Move"),
        shortcuts::separator(theme),
        shortcuts::esc_cancel(theme),
    ];
    let instructions_paragraph = common::create_instructions_paragraph(&instructions, theme);

    f.render_widget(main_block, dialog_area);
    f.render_widget(input_paragraph, chunks[0]);
    f.render_stateful_widget(list, chunks[1], &mut list_state);
    f.render_widget(instructions_paragraph, chunks[2]);

    f.set_cursor_position((chunks[0].x + 1 + view.cursor as u16, chunks[0].y + 1));
}
