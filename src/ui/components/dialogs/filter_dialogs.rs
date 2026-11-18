use super::common::{self, shortcuts};
use crate::theme::Theme;
use crate::ui::core::project_filter::PickerEntry;
use crate::ui::layout::LayoutManager;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState},
    Frame,
};
use uuid::Uuid;

/// The project filter picker: a query field over a list of matching projects.
#[allow(clippy::too_many_arguments)]
pub fn render_project_filter_dialog(
    f: &mut Frame,
    area: Rect,
    query: &str,
    cursor_position: usize,
    entries: &[PickerEntry],
    selected: usize,
    current: Option<Uuid>,
    theme: &Theme,
) {
    // Tall enough for a useful list, never taller than the screen.
    let lines = area.height.saturating_sub(4).clamp(10, 24);
    let dialog_area = LayoutManager::centered_rect_lines(50, lines, area);
    f.render_widget(Clear, dialog_area);

    let main_block = common::create_dialog_block("Filter by Project", theme.info);
    let inner_area = main_block.inner(dialog_area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Query field
            Constraint::Min(3),    // Project list
            Constraint::Length(1), // Instructions
        ])
        .split(inner_area);

    let input_paragraph = common::create_input_paragraph(query, cursor_position, "Type to narrow", theme);

    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let marker = if entry.project == current { "● " } else { "  " };
            let indent = "  ".repeat(entry.depth);
            let prefix = if entry.project.is_some() { "#" } else { "" };
            ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(theme.info)),
                Span::styled(
                    format!("{indent}{prefix}{}", entry.name),
                    Style::default().fg(theme.text),
                ),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Projects ")
                .title_style(Style::default().fg(theme.text))
                .border_style(Style::default().fg(theme.border_dim)),
        )
        .highlight_style(Style::default().bg(theme.selection_bg).add_modifier(Modifier::BOLD));
    let mut list_state = ListState::default();
    list_state.select(Some(selected.min(entries.len().saturating_sub(1))));

    let instructions = [
        ("Enter", theme.success, " Apply"),
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

    f.set_cursor_position((chunks[0].x + 1 + cursor_position as u16, chunks[0].y + 1));
}
