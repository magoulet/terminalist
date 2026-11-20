use super::common::{self, shortcuts};
use super::text_edit;
use crate::theme::Theme;
use crate::ui::components::dialog_component::TaskField;
use crate::ui::layout::LayoutManager;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

/// Rows of description text visible at once. Longer descriptions scroll with the cursor.
const DESCRIPTION_ROWS: u16 = 6;

/// What the task create and edit dialogs show.
pub struct TaskDialogView<'a> {
    pub content: &'a str,
    pub content_cursor: usize,
    pub description: &'a str,
    pub description_cursor: usize,
    pub focus: TaskField,
    pub project_name: String,
    pub is_editing: bool,
    /// Creating a subtask: `project_name` holds the parent's title, which can't be changed.
    pub is_subtask: bool,
}

/// A field's frame, brighter while it has focus.
fn field_block<'a>(title: &str, focused: bool, theme: &Theme) -> Block<'a> {
    let border = if focused { theme.info } else { theme.border_dim };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(format!(" {title} "))
        .title_style(Style::default().fg(theme.text))
        .border_style(Style::default().fg(border))
}

/// Draws the task dialog and returns the description field's text width, which the dialog
/// needs to move the cursor up and down a wrapped row.
pub fn render_task_dialog(f: &mut Frame, area: Rect, view: &TaskDialogView, theme: &Theme) -> usize {
    let title = if view.is_editing {
        "Edit Task"
    } else if view.is_subtask {
        "New Subtask"
    } else {
        "New Task"
    };
    let dialog_area = LayoutManager::centered_rect_lines(65, 21, area);
    f.render_widget(Clear, dialog_area);

    let main_block = common::create_dialog_block(title, theme.info);
    let inner_area = main_block.inner(dialog_area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),                    // Title field
            Constraint::Length(DESCRIPTION_ROWS + 2), // Description field
            Constraint::Length(3),                    // Project field
            Constraint::Length(1),                    // Spacer
            Constraint::Length(1),                    // Instructions
            Constraint::Length(1),                    // More instructions
        ])
        .split(inner_area);

    // New tasks go through Todoist Quick Add, which parses these out of the text.
    let content_label = if view.is_editing {
        "Task Content"
    } else {
        "Task (try: tomorrow 3pm #Project @label p1)"
    };
    let content = Paragraph::new(view.content)
        .block(field_block(content_label, view.focus == TaskField::Content, theme))
        .style(Style::default().fg(theme.text));

    // The description wraps by hand so the cursor position is known, and scrolls so the
    // cursor row stays in view.
    let width = usize::from(chunks[1].width.saturating_sub(2)).max(1);
    let rows = text_edit::wrap(view.description, width);
    let (cursor_row, cursor_column) = text_edit::cursor_cell(&rows, view.description_cursor);
    let visible = usize::from(DESCRIPTION_ROWS);
    let scroll = cursor_row.saturating_sub(visible - 1);
    let chars: Vec<char> = view.description.chars().collect();
    let lines: Vec<Line> = rows
        .iter()
        .skip(scroll)
        .take(visible)
        .map(|row| Line::from(chars[row.start..row.start + row.len].iter().collect::<String>()))
        .collect();
    let description = Paragraph::new(lines)
        .block(field_block("Description", view.focus == TaskField::Description, theme))
        .style(Style::default().fg(theme.text));

    let project_focused = view.focus == TaskField::Project;
    let (project_label, project_text) = if view.is_subtask {
        ("Subtask of", view.project_name.clone())
    } else if project_focused {
        ("Project", format!("◀ {} ▶", view.project_name))
    } else {
        ("Project", view.project_name.clone())
    };
    let project = Paragraph::new(project_text)
        .block(field_block(project_label, project_focused, theme))
        .style(Style::default().fg(theme.text));

    let save = if view.is_editing { " Save Task" } else { " Create Task" };
    let instructions = [
        ("Enter", theme.success, save),
        shortcuts::separator(theme),
        ("Tab", theme.info, " Next field"),
        shortcuts::separator(theme),
        shortcuts::esc_cancel(theme),
    ];
    let more_instructions = [
        ("Alt+Enter", theme.info, " New line in description"),
        shortcuts::separator(theme),
        ("←/→", theme.info, " Change project"),
    ];

    f.render_widget(main_block, dialog_area);
    f.render_widget(content, chunks[0]);
    f.render_widget(description, chunks[1]);
    f.render_widget(project, chunks[2]);
    f.render_widget(common::create_instructions_paragraph(&instructions, theme), chunks[4]);
    f.render_widget(
        common::create_instructions_paragraph(&more_instructions, theme),
        chunks[5],
    );

    // Only the text fields show a cursor.
    match view.focus {
        TaskField::Content => {
            f.set_cursor_position((chunks[0].x + 1 + view.content_cursor as u16, chunks[0].y + 1));
        }
        TaskField::Description => {
            f.set_cursor_position((
                chunks[1].x + 1 + cursor_column.min(width - 1) as u16,
                chunks[1].y + 1 + (cursor_row - scroll) as u16,
            ));
        }
        TaskField::Project => {}
    }

    width
}

/// The reschedule dialog: one line of date text that Todoist parses.
pub fn render_reschedule_dialog(
    f: &mut Frame,
    area: Rect,
    input: &str,
    cursor: usize,
    task_content: &str,
    is_recurring: bool,
    theme: &Theme,
) {
    let dialog_area = LayoutManager::centered_rect_lines(65, 11, area);
    f.render_widget(Clear, dialog_area);

    let main_block = common::create_dialog_block("Reschedule Task", theme.info);
    let inner_area = main_block.inner(dialog_area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // Task being rescheduled
            Constraint::Length(1), // Repeat warning
            Constraint::Length(3), // Date input
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Instructions
        ])
        .split(inner_area);

    let task_line = Paragraph::new(task_content.to_string()).style(Style::default().fg(theme.text_muted));
    let input_paragraph = common::create_input_paragraph(
        input,
        cursor,
        "New date (fri 3pm, in 2 weeks, every monday, no date)",
        theme,
    );
    let instructions = [
        ("Enter", theme.success, " Reschedule"),
        shortcuts::separator(theme),
        shortcuts::esc_cancel(theme),
    ];

    f.render_widget(main_block, dialog_area);
    f.render_widget(task_line, chunks[0]);
    if is_recurring {
        // Todoist replaces the whole due date, repeat pattern included.
        let warning =
            Paragraph::new("This task repeats. A plain date ends the repeat, so type \"every ...\" to keep it.")
                .style(Style::default().fg(theme.warning));
        f.render_widget(warning, chunks[1]);
    }
    f.render_widget(input_paragraph, chunks[2]);
    f.render_widget(common::create_instructions_paragraph(&instructions, theme), chunks[4]);

    f.set_cursor_position((chunks[2].x + 1 + cursor as u16, chunks[2].y + 1));
}
