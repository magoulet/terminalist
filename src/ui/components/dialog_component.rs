//! Modal dialog component for various user interactions.
//!
//! This component provides a flexible modal dialog system that handles different
//! types of user interactions including task creation/editing, project management,
//! label management, and system functions like search and debugging.

use crate::config::DisplayConfig;
use crate::entities::{label, project, task};
use crate::sync::SyncService;
use crate::theme::Theme;
use crate::ui::components::task_list_item_component::{ListItem as TaskListItem, TaskItem};
use crate::ui::core::{
    actions::{Action, DialogType},
    Component,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, widgets::ScrollbarState, Frame};
use uuid::Uuid;

use crate::ui::components::dialogs::{
    filter_dialogs, label_dialogs, project_dialogs, scroll_behavior, system_dialogs, task_dialogs, text_edit,
};
use crate::ui::core::project_filter::{self, PickerEntry};
use crate::ui::core::subtasks::{self, ParentChoice};

/// The field of the task create and edit dialogs that has the keyboard. Tab moves forward,
/// Shift+Tab back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TaskField {
    #[default]
    Content,
    Description,
    Project,
}

impl TaskField {
    fn next(self) -> Self {
        match self {
            TaskField::Content => TaskField::Description,
            TaskField::Description => TaskField::Project,
            TaskField::Project => TaskField::Content,
        }
    }

    fn previous(self) -> Self {
        match self {
            TaskField::Content => TaskField::Project,
            TaskField::Description => TaskField::Content,
            TaskField::Project => TaskField::Description,
        }
    }
}

/// What a key did in a picker dialog.
enum Picked {
    Nothing,
    Cancel,
    Row(usize),
}

/// Modal dialog component that handles various user interactions.
///
/// This component serves as a container for different types of dialogs:
///
/// # Dialog Types
/// - **Task dialogs** - Create, edit, and manage tasks
/// - **Project dialogs** - Create and manage projects
/// - **Label dialogs** - Create and manage labels
/// - **System dialogs** - Search, logs, help, and confirmation dialogs
///
/// # Features
/// - Input handling with cursor management
/// - Scrolling support for long content
/// - Project/label selection interfaces
/// - Search functionality with live results
/// - Integration with sync service for immediate updates
/// - Configurable display options
///
/// The component delegates specific dialog rendering and logic to specialized
/// dialog modules while providing common infrastructure like input handling
/// and state management.
pub struct DialogComponent {
    pub dialog_type: Option<DialogType>,
    pub input_buffer: String,
    pub cursor_position: usize,
    pub projects: Vec<project::Model>,
    pub labels: Vec<label::Model>,
    pub tasks: Vec<task::Model>,
    pub selected_project_index: usize,
    pub selected_parent_project_index: Option<usize>, // For project creation parent selection
    pub selected_task_project_uuid: Option<Uuid>,     // Selected project in the task dialog (None = Inbox)
    pub task_project_explicitly_selected: bool,       // Track if user explicitly changed the project
    // Task dialogs: the multi-line description and which field has focus
    pub description_buffer: String,
    pub description_cursor: usize,
    pub task_field: TaskField,
    /// Text width of the description field at the last draw, for moving up and down a row.
    pub description_width: usize,
    // Scrolling support for long content dialogs
    pub scroll_offset: usize,
    pub scrollbar_state: ScrollbarState,
    // Task search state
    pub search_results: Vec<task::Model>,
    pub sync_service: Option<SyncService>,
    pub display_config: DisplayConfig,
    pub theme: Theme,
}

impl Default for DialogComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl DialogComponent {
    pub fn new() -> Self {
        Self {
            dialog_type: None,
            input_buffer: String::new(),
            cursor_position: 0,
            projects: Vec::new(),
            labels: Vec::new(),
            tasks: Vec::new(),
            selected_project_index: 0,
            selected_parent_project_index: None,
            selected_task_project_uuid: None, // No project selected initially (Inbox)
            task_project_explicitly_selected: false, // User hasn't changed the project yet
            description_buffer: String::new(),
            description_cursor: 0,
            task_field: TaskField::Content,
            description_width: 40,
            scroll_offset: 0,
            scrollbar_state: ScrollbarState::new(0),
            search_results: Vec::new(),
            sync_service: None,
            display_config: DisplayConfig::default(),
            theme: Theme::default(),
        }
    }

    pub fn update_display_config(&mut self, display_config: DisplayConfig) {
        self.display_config = display_config;
    }

    pub fn update_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn update_data_with_tasks(
        &mut self,
        projects: Vec<project::Model>,
        labels: Vec<label::Model>,
        tasks: Vec<task::Model>,
    ) {
        self.projects = projects;
        self.labels = labels;
        self.tasks = tasks;
    }

    pub fn set_sync_service(&mut self, sync_service: SyncService) {
        self.sync_service = Some(sync_service);
    }

    /// Get root projects (projects without a parent) for parent selection
    pub fn get_root_projects(&self) -> Vec<&project::Model> {
        self.projects.iter().filter(|project| project.parent_uuid.is_none()).collect()
    }

    /// Get all non-inbox projects for task creation (excludes inbox project)
    pub fn get_task_projects(&self) -> Vec<&project::Model> {
        self.projects.iter().filter(|project| !project.is_inbox_project).collect()
    }

    /// Rows of the project filter picker for the text typed so far.
    fn filter_entries(&self) -> Vec<PickerEntry> {
        project_filter::picker_entries(&self.projects, &self.input_buffer)
    }

    /// Rows of the "move under" picker for the text typed so far.
    fn parent_entries(&self) -> Vec<ParentChoice> {
        match &self.dialog_type {
            Some(DialogType::ParentPicker {
                task_uuid, candidates, ..
            }) => subtasks::parent_choices(candidates, *task_uuid, &self.input_buffer),
            _ => Vec::new(),
        }
    }

    /// Keys shared by the picker dialogs. Typing narrows the list, the arrows move the
    /// cursor (`selected_project_index`), Enter picks the row under it. `count` gives the
    /// number of rows for the text typed so far. Returns the picked row on Enter.
    fn handle_picker_key(&mut self, code: KeyCode, count: impl Fn(&Self) -> usize) -> Picked {
        match code {
            KeyCode::Esc => Picked::Cancel,
            KeyCode::Enter => Picked::Row(self.selected_project_index),
            KeyCode::Up => {
                self.selected_project_index = self.selected_project_index.saturating_sub(1);
                Picked::Nothing
            }
            KeyCode::Down => {
                if self.selected_project_index + 1 < count(self) {
                    self.selected_project_index += 1;
                }
                Picked::Nothing
            }
            code => {
                if self.handle_input_key(code) {
                    // Jump to the first match rather than the fixed first row.
                    self.selected_project_index = usize::from(!self.input_buffer.is_empty() && count(self) > 1);
                }
                Picked::Nothing
            }
        }
    }

    /// Keys for the project filter picker.
    fn handle_filter_key(&mut self, code: KeyCode) -> Action {
        match self.handle_picker_key(code, |dialog| dialog.filter_entries().len()) {
            Picked::Cancel => Action::HideDialog,
            Picked::Row(index) => {
                let chosen = self.filter_entries().get(index).and_then(|entry| entry.project);
                self.clear_dialog();
                Action::SetProjectFilter(chosen)
            }
            Picked::Nothing => Action::None,
        }
    }

    /// Keys for the "move under" picker. Picking the parent the task already has does nothing.
    fn handle_parent_key(&mut self, code: KeyCode) -> Action {
        match self.handle_picker_key(code, |dialog| dialog.parent_entries().len()) {
            Picked::Cancel => Action::HideDialog,
            Picked::Row(index) => {
                let Some(choice) = self.parent_entries().get(index).cloned() else {
                    return Action::None;
                };
                let action = match &self.dialog_type {
                    Some(DialogType::ParentPicker {
                        task_uuid,
                        current_parent,
                        ..
                    }) if choice.parent != *current_parent => Action::SetTaskParent {
                        task_uuid: *task_uuid,
                        parent: choice.parent,
                    },
                    _ => Action::HideDialog,
                };
                self.clear_dialog();
                action
            }
            Picked::Nothing => Action::None,
        }
    }

    /// Trigger a database search based on current input
    fn trigger_search(&mut self) -> Action {
        // Trigger background database search
        Action::SearchTasks(self.input_buffer.clone())
    }

    /// Update search results from database query results
    pub fn update_search_results(&mut self, query: &str, results: Vec<task::Model>) {
        // Only update if this is for the current query (avoid race conditions)
        if query == self.input_buffer {
            self.search_results = results;
        }
    }

    pub fn is_visible(&self) -> bool {
        self.dialog_type.is_some()
    }

    fn handle_submit(&mut self) -> Action {
        match &self.dialog_type {
            Some(DialogType::TaskCreation {
                default_project_uuid,
                parent,
            }) => {
                if !self.input_buffer.is_empty() {
                    // Determine the project UUID based on whether user explicitly selected via Tab
                    let project_uuid = if self.task_project_explicitly_selected {
                        // User pressed Tab - use their selection (could be None for Inbox or Some(uuid) for a project)
                        self.selected_task_project_uuid
                    } else {
                        // User didn't press Tab - use default project
                        *default_project_uuid
                    };

                    // Debug logging
                    if let Some(ref pid) = project_uuid {
                        let proj_name = self
                            .projects
                            .iter()
                            .find(|p| &p.uuid == pid)
                            .map(|p| p.name.as_str())
                            .unwrap_or("unknown");
                        log::info!("Creating task in project: {} ({})", proj_name, pid);
                    } else {
                        log::info!("Creating task in inbox (no project)");
                    }

                    let description = Some(self.description_buffer.trim_end().to_string()).filter(|d| !d.is_empty());
                    let action = Action::CreateTask {
                        content: self.input_buffer.clone(),
                        project_uuid,
                        description,
                        parent_uuid: parent.as_ref().map(|(uuid, _)| *uuid),
                    };
                    self.clear_dialog();
                    action
                } else {
                    Action::None
                }
            }
            Some(DialogType::TaskEdit {
                task_uuid,
                project_uuid,
                description,
                ..
            }) => {
                if !self.input_buffer.is_empty() {
                    // Only move when the project was changed to one other than the task's own
                    let move_to_project = self.selected_task_project_uuid.filter(|uuid| uuid != project_uuid);
                    // Only send the description when it changed
                    let edited = self.description_buffer.trim_end();
                    let description = (edited != description.trim_end()).then(|| edited.to_string());
                    let action = Action::EditTask {
                        task_uuid: *task_uuid,
                        content: self.input_buffer.clone(),
                        move_to_project,
                        description,
                    };
                    self.clear_dialog();
                    action
                } else {
                    Action::None
                }
            }
            Some(DialogType::GoToCompletedDay) => {
                let typed = self.input_buffer.trim().to_string();
                if typed.is_empty() {
                    return Action::None;
                }
                self.clear_dialog();
                match crate::utils::datetime::parse_day(&typed, chrono::Local::now().date_naive()) {
                    Some(day) => Action::ShowCompletedDay(day),
                    None => Action::ShowDialog(DialogType::Error(format!(
                        "Couldn't read '{typed}' as a day. Try yesterday, 3 days ago, monday, nov 2 or 2026-11-02."
                    ))),
                }
            }
            Some(DialogType::Reschedule { task_uuid, .. }) => {
                let due = self.input_buffer.trim().to_string();
                if due.is_empty() {
                    Action::None
                } else {
                    let action = Action::RescheduleTask {
                        task_uuid: *task_uuid,
                        due,
                    };
                    self.clear_dialog();
                    action
                }
            }
            Some(DialogType::ProjectCreation) => {
                if !self.input_buffer.is_empty() {
                    let parent_uuid = if let Some(parent_index) = self.selected_parent_project_index {
                        let root_projects = self.get_root_projects();
                        if parent_index < root_projects.len() {
                            Some(root_projects[parent_index].uuid)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let action = Action::CreateProject {
                        name: self.input_buffer.clone(),
                        parent_uuid,
                    };
                    self.clear_dialog();
                    action
                } else {
                    Action::None
                }
            }
            Some(DialogType::ProjectEdit { project_uuid, .. }) => {
                if !self.input_buffer.is_empty() {
                    let action = Action::EditProject {
                        project_uuid: *project_uuid,
                        name: self.input_buffer.clone(),
                    };
                    self.clear_dialog();
                    action
                } else {
                    Action::None
                }
            }
            Some(DialogType::LabelCreation) => {
                if !self.input_buffer.is_empty() {
                    let action = Action::CreateLabel {
                        name: self.input_buffer.clone(),
                    };
                    self.clear_dialog();
                    action
                } else {
                    Action::None
                }
            }
            Some(DialogType::LabelEdit { label_uuid, .. }) => {
                if !self.input_buffer.is_empty() {
                    let action = Action::EditLabel {
                        label_uuid: *label_uuid,
                        name: self.input_buffer.clone(),
                    };
                    self.clear_dialog();
                    action
                } else {
                    Action::None
                }
            }
            Some(DialogType::DeleteConfirmation { item_type, item_uuid }) => match item_type.as_str() {
                "task" => {
                    let action = Action::DeleteTask(*item_uuid);
                    self.clear_dialog();
                    action
                }
                "project" => {
                    let action = Action::DeleteProject(*item_uuid);
                    self.clear_dialog();
                    action
                }
                "label" => {
                    let action = Action::DeleteLabel(*item_uuid);
                    self.clear_dialog();
                    action
                }
                _ => Action::None,
            },
            _ => Action::None,
        }
    }

    fn clear_dialog(&mut self) {
        self.dialog_type = None;
        self.input_buffer.clear();
        self.cursor_position = 0;
        self.selected_project_index = 0;
        self.selected_parent_project_index = None;
        self.selected_task_project_uuid = None; // Reset to Inbox for task creation
        self.task_project_explicitly_selected = false; // Reset selection flag
        self.description_buffer.clear();
        self.description_cursor = 0;
        self.task_field = TaskField::Content;
        self.scroll_offset = 0;
        self.scrollbar_state = ScrollbarState::new(0);
        self.search_results.clear();
    }

    fn scroll_up(&mut self) {
        scroll_behavior::scroll_up(&mut self.scroll_offset, &mut self.scrollbar_state);
    }

    fn scroll_down(&mut self) {
        scroll_behavior::scroll_down(&mut self.scroll_offset, &mut self.scrollbar_state);
    }

    fn page_up(&mut self) {
        scroll_behavior::page_up(&mut self.scroll_offset, &mut self.scrollbar_state);
    }

    fn page_down(&mut self) {
        scroll_behavior::page_down(&mut self.scroll_offset, &mut self.scrollbar_state);
    }

    fn scroll_to_top(&mut self) {
        scroll_behavior::scroll_to_top(&mut self.scroll_offset, &mut self.scrollbar_state);
    }

    fn scroll_to_bottom(&mut self) {
        scroll_behavior::scroll_to_bottom(&mut self.scroll_offset, &mut self.scrollbar_state);
    }

    fn render_task_creation_dialog(&mut self, f: &mut Frame, area: Rect) {
        if let Some(DialogType::TaskCreation {
            parent: Some((_, parent)),
            ..
        }) = &self.dialog_type
        {
            let parent = parent.clone();
            self.render_task_dialog(f, area, parent, false, true);
            return;
        }
        let project_name = self
            .selected_task_project_uuid
            .and_then(|uuid| self.get_task_projects().into_iter().find(|p| p.uuid == uuid))
            .map_or_else(|| "None (Inbox)".to_string(), |p| p.name.clone());
        self.render_task_dialog(f, area, project_name, false, false);
    }

    /// Draws either task dialog and keeps the description width for moving up and down rows.
    /// For a subtask, `project_name` holds the parent's title instead.
    fn render_task_dialog(
        &mut self,
        f: &mut Frame,
        area: Rect,
        project_name: String,
        is_editing: bool,
        is_subtask: bool,
    ) {
        let view = task_dialogs::TaskDialogView {
            content: &self.input_buffer,
            content_cursor: self.cursor_position,
            description: &self.description_buffer,
            description_cursor: self.description_cursor,
            focus: self.task_field,
            project_name,
            is_editing,
            is_subtask,
        };
        self.description_width = task_dialogs::render_task_dialog(f, area, &view, &self.theme);
    }

    fn render_project_creation_dialog(&self, f: &mut Frame, area: Rect) {
        let root_projects = self.get_root_projects();
        project_dialogs::render_project_creation_dialog(
            f,
            area,
            &self.input_buffer,
            self.cursor_position,
            &root_projects,
            self.selected_parent_project_index,
            &self.theme,
        );
    }

    fn render_project_edit_dialog(&self, f: &mut Frame, area: Rect) {
        project_dialogs::render_project_edit_dialog(f, area, &self.input_buffer, self.cursor_position, &self.theme);
    }

    fn render_label_creation_dialog(&self, f: &mut Frame, area: Rect) {
        label_dialogs::render_label_dialog(
            f,
            area,
            &self.input_buffer,
            self.cursor_position,
            false, // is_editing
            &self.theme,
        );
    }

    fn render_label_edit_dialog(&self, f: &mut Frame, area: Rect) {
        label_dialogs::render_label_dialog(
            f,
            area,
            &self.input_buffer,
            self.cursor_position,
            true, // is_editing
            &self.theme,
        );
    }

    fn render_task_edit_dialog(&mut self, f: &mut Frame, area: Rect) {
        // Editing cycles through every project, inbox included
        let project_name = self
            .selected_task_project_uuid
            .and_then(|uuid| self.projects.iter().find(|p| p.uuid == uuid))
            .map_or_else(|| "None (Inbox)".to_string(), |p| p.name.clone());
        self.render_task_dialog(f, area, project_name, true, false);
    }

    fn render_delete_confirmation_dialog(&self, f: &mut Frame, area: Rect, item_type: &str) {
        system_dialogs::render_delete_confirmation_dialog(f, area, item_type, &self.theme);
    }

    fn render_info_dialog(&mut self, f: &mut Frame, area: Rect, message: &str) {
        system_dialogs::render_info_dialog(
            f,
            area,
            message,
            self.scroll_offset,
            &mut self.scrollbar_state,
            &self.theme,
        );
    }

    fn render_error_dialog(&mut self, f: &mut Frame, area: Rect, message: &str) {
        system_dialogs::render_error_dialog(
            f,
            area,
            message,
            self.scroll_offset,
            &mut self.scrollbar_state,
            &self.theme,
        );
    }

    fn render_help_dialog(&mut self, f: &mut Frame, area: Rect) {
        system_dialogs::render_help_dialog(f, area, self.scroll_offset, &mut self.scrollbar_state, &self.theme);
    }

    fn render_task_search_dialog(&self, f: &mut Frame, area: Rect) {
        use ratatui::{
            layout::{Constraint, Layout, Margin},
            style::Style,
            widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
        };

        // Create a centered popup area
        let popup_area = {
            let popup_layout =
                Layout::vertical([Constraint::Percentage(10), Constraint::Min(20), Constraint::Percentage(10)])
                    .split(area);

            Layout::horizontal([Constraint::Percentage(10), Constraint::Min(60), Constraint::Percentage(10)])
                .split(popup_layout[1])[1]
        };

        // Clear the area
        f.render_widget(Clear, popup_area);

        // Split into input area and results area
        let content_area = popup_area.inner(Margin {
            horizontal: 1,
            vertical: 1,
        });

        let layout = Layout::vertical([
            Constraint::Length(3), // Input area
            Constraint::Min(0),    // Results area
        ])
        .split(content_area);

        // Render the main block
        let main_block = Block::default()
            .title(" Search Tasks ")
            .borders(Borders::ALL)
            .style(Style::default().fg(self.theme.border_dim));
        f.render_widget(main_block, popup_area);

        // Render input field
        let input_paragraph = Paragraph::new(self.input_buffer.as_str()).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Query")
                .style(Style::default().fg(self.theme.border_dim)),
        );
        f.render_widget(input_paragraph, layout[0]);

        // Set cursor position in input field
        f.set_cursor_position((layout[0].x + 1 + self.cursor_position as u16, layout[0].y + 1));

        // Render search results
        let results_text = if self.search_results.is_empty() {
            if self.input_buffer.is_empty() {
                "Start typing to search tasks…".to_string()
            } else {
                "No tasks found.".to_string()
            }
        } else {
            format!("{} tasks found", self.search_results.len())
        };

        let results_list: Vec<ListItem> = self
            .search_results
            .iter()
            .map(|task| {
                // Resolve the task's project name once, at build time
                let project_name = self
                    .projects
                    .iter()
                    .find(|p| p.uuid == task.project_uuid)
                    .map(|p| p.name.clone());

                // Create TaskItem with the same formatting as main task list
                // TODO: Load task-label relationships from database
                let task_item = TaskItem::new(
                    task.clone(),
                    0, // depth: 0 for search results (no indentation)
                    0, // child_count: 0 for search results
                    project_name,
                    Vec::new(),
                );

                // Use the same render method as main task list
                TaskListItem::render(&task_item, false, &self.display_config, &self.theme)
            })
            .collect();

        let results_block = Block::default()
            .borders(Borders::ALL)
            .title(results_text)
            .style(Style::default().fg(self.theme.border_dim));

        let results_list_widget = List::new(results_list).block(results_block);
        f.render_widget(results_list_widget, layout[1]);
    }

    fn render_logs_dialog(&mut self, f: &mut Frame, area: Rect) {
        system_dialogs::render_logs_dialog(f, area, self.scroll_offset, &mut self.scrollbar_state, &self.theme);
    }
}

/// Move a cursor over `len` items one step forward or back, through a virtual "none" slot
/// after the last one.
fn step(current: Option<usize>, len: usize, forward: bool) -> Option<usize> {
    let slot = current.unwrap_or(len);
    let next = if forward {
        (slot + 1) % (len + 1)
    } else {
        (slot + len) % (len + 1)
    };
    (next < len).then_some(next)
}

impl DialogComponent {
    /// The scroll keys shared by the info, error, help and logs dialogs. Returns whether the
    /// key was one of them.
    fn handle_scroll_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Up | KeyCode::Char('k') => self.scroll_up(),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_down(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::Home => self.scroll_to_top(),
            KeyCode::End => self.scroll_to_bottom(),
            _ => return false,
        }
        true
    }

    /// The text-editing keys shared by every input dialog. Returns whether the text changed,
    /// which is what tells the search dialog to run a new query.
    fn handle_input_key(&mut self, key: KeyCode) -> bool {
        text_edit::edit(&mut self.input_buffer, &mut self.cursor_position, key)
    }

    /// Keys for the task create and edit dialogs. Tab and Shift+Tab move between the title,
    /// description and project fields. Enter saves from any field. Alt+Enter starts a new
    /// line in the description, since most terminals send Shift+Enter as a plain Enter.
    fn handle_task_dialog_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => return Action::HideDialog,
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
                if self.task_field == TaskField::Description {
                    text_edit::insert_newline(&mut self.description_buffer, &mut self.description_cursor);
                }
                return Action::None;
            }
            KeyCode::Enter => return self.handle_submit(),
            KeyCode::Tab => {
                self.task_field = self.task_field.next();
                return Action::None;
            }
            KeyCode::BackTab => {
                self.task_field = self.task_field.previous();
                return Action::None;
            }
            _ => {}
        }
        match self.task_field {
            TaskField::Content => {
                self.handle_input_key(key.code);
            }
            TaskField::Description => match key.code {
                KeyCode::Up | KeyCode::Down => {
                    let width = self.description_width;
                    text_edit::move_vertical(
                        &self.description_buffer,
                        &mut self.description_cursor,
                        width,
                        key.code == KeyCode::Down,
                    );
                }
                code => {
                    text_edit::edit(&mut self.description_buffer, &mut self.description_cursor, code);
                }
            },
            TaskField::Project => match key.code {
                KeyCode::Right | KeyCode::Down | KeyCode::Char(' ') => self.cycle_project_selection(true),
                KeyCode::Left | KeyCode::Up => self.cycle_project_selection(false),
                _ => {}
            },
        }
        Action::None
    }

    /// Steps the project selector of whichever dialog is open, forward or back, wrapping.
    fn cycle_project_selection(&mut self, forward: bool) {
        match self.dialog_type {
            // Editing: every project is a valid destination, inbox included, and the task
            // always sits in one of them.
            Some(DialogType::TaskEdit { .. }) if !self.projects.is_empty() => {
                let len = self.projects.len();
                let current = self
                    .selected_task_project_uuid
                    .and_then(|uuid| self.projects.iter().position(|project| project.uuid == uuid));
                let next = match current {
                    Some(index) if forward => (index + 1) % len,
                    Some(index) => (index + len - 1) % len,
                    None => 0,
                };
                self.selected_task_project_uuid = Some(self.projects[next].uuid);
            }
            // Creating: the inbox is the "none" slot at the end of the cycle.
            // A subtask lives in its parent's project, so there is nothing to choose.
            Some(DialogType::TaskCreation { parent: Some(_), .. }) => {}
            Some(DialogType::TaskCreation { .. }) => {
                let uuids: Vec<Uuid> = self.get_task_projects().iter().map(|project| project.uuid).collect();
                self.task_project_explicitly_selected = true;
                let current = self
                    .selected_task_project_uuid
                    .and_then(|uuid| uuids.iter().position(|candidate| *candidate == uuid));
                self.selected_task_project_uuid = step(current, uuids.len(), forward).map(|next| uuids[next]);
            }
            Some(DialogType::ProjectCreation) => {
                self.selected_parent_project_index = step(
                    self.selected_parent_project_index,
                    self.get_root_projects().len(),
                    forward,
                );
            }
            _ => {}
        }
    }
}

impl Component for DialogComponent {
    fn handle_key_events(&mut self, key: KeyEvent) -> Action {
        match &self.dialog_type {
            None => Action::None,
            // Scrollable dialogs. Any key that does not scroll dismisses the info and error
            // ones; help and logs keep their own dismiss keys and ignore the rest.
            Some(DialogType::Info(_) | DialogType::Error(_)) => {
                if self.handle_scroll_key(key.code) {
                    Action::None
                } else {
                    Action::HideDialog
                }
            }
            Some(DialogType::Help) => match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('h') => Action::HideDialog,
                code => {
                    self.handle_scroll_key(code);
                    Action::None
                }
            },
            Some(DialogType::Logs) => match key.code {
                KeyCode::Esc | KeyCode::Char('!') | KeyCode::Char('q') => Action::HideDialog,
                code => {
                    self.handle_scroll_key(code);
                    Action::None
                }
            },
            Some(DialogType::DeleteConfirmation { .. }) => match key.code {
                KeyCode::Esc => Action::HideDialog,
                KeyCode::Enter => self.handle_submit(),
                _ => Action::None,
            },
            // The search dialog edits text like the input dialogs, but every change runs a
            // new query instead of waiting for Enter.
            Some(DialogType::TaskSearch) => match key.code {
                KeyCode::Esc | KeyCode::Enter => Action::HideDialog,
                code if self.handle_input_key(code) => self.trigger_search(),
                _ => Action::None,
            },
            Some(DialogType::ProjectFilter { .. }) => self.handle_filter_key(key.code),
            Some(DialogType::ParentPicker { .. }) => self.handle_parent_key(key.code),
            Some(DialogType::TaskCreation { .. } | DialogType::TaskEdit { .. }) => self.handle_task_dialog_key(key),
            // Input dialogs: task, project and label creation and editing.
            _ => match key.code {
                KeyCode::Esc => Action::HideDialog,
                KeyCode::Enter => self.handle_submit(),
                KeyCode::Tab => {
                    self.cycle_project_selection(true);
                    Action::None
                }
                code => {
                    self.handle_input_key(code);
                    Action::None
                }
            },
        }
    }

    fn update(&mut self, action: Action) -> Action {
        match action {
            Action::ShowDialog(dialog_type) => {
                // Check if this is a task creation dialog before moving the value
                let is_task_creation = matches!(dialog_type, DialogType::TaskCreation { .. });

                // Pre-populate input for edit dialogs
                match &dialog_type {
                    DialogType::TaskEdit {
                        content,
                        description,
                        project_uuid,
                        ..
                    } => {
                        self.input_buffer = content.clone();
                        self.cursor_position = content.chars().count();
                        self.description_buffer = description.clone();
                        self.description_cursor = description.chars().count();
                        self.task_field = TaskField::Content;
                        self.selected_task_project_uuid = Some(*project_uuid);
                    }
                    DialogType::ProjectEdit { name, .. } => {
                        self.input_buffer = name.clone();
                        self.cursor_position = name.chars().count();
                    }
                    DialogType::LabelEdit { name, .. } => {
                        self.input_buffer = name.clone();
                        self.cursor_position = name.chars().count();
                    }
                    DialogType::TaskCreation {
                        default_project_uuid, ..
                    } => {
                        self.input_buffer.clear();
                        self.cursor_position = 0;
                        self.description_buffer.clear();
                        self.description_cursor = 0;
                        self.task_field = TaskField::Content;
                        // Preselect the default project when one is provided
                        if let Some(project_uuid) = default_project_uuid {
                            let task_projects = self.get_task_projects();
                            if task_projects.iter().any(|p| &p.uuid == project_uuid) {
                                self.selected_task_project_uuid = Some(*project_uuid);
                                let proj_name = self
                                    .projects
                                    .iter()
                                    .find(|p| &p.uuid == project_uuid)
                                    .map(|p| p.name.as_str())
                                    .unwrap_or("unknown");
                                log::info!("Dialog opened with default project: {} ({})", proj_name, project_uuid);
                            }
                        } else {
                            log::info!("Dialog opened with no default project (inbox)");
                        }
                    }
                    DialogType::TaskSearch => {
                        self.input_buffer.clear();
                        self.cursor_position = 0;
                        self.search_results.clear();
                    }
                    _ => {
                        self.input_buffer.clear();
                        self.cursor_position = 0;
                    }
                }
                self.dialog_type = Some(dialog_type.clone());
                // Only reset project index for non-task-creation dialogs
                if !is_task_creation {
                    self.selected_project_index = 0;
                }

                // Open the filter picker on the row of the filter already in place.
                if let DialogType::ParentPicker { current_parent, .. } = &dialog_type {
                    let current_parent = *current_parent;
                    self.selected_project_index = self
                        .parent_entries()
                        .iter()
                        .position(|choice| choice.parent == current_parent)
                        .unwrap_or(0);
                }
                if let DialogType::ProjectFilter { current } = dialog_type {
                    self.selected_project_index = self
                        .filter_entries()
                        .iter()
                        .position(|entry| entry.project == current)
                        .unwrap_or(0);
                }

                // Trigger initial search for TaskSearch dialog
                if matches!(dialog_type, DialogType::TaskSearch) {
                    return self.trigger_search();
                }

                Action::None
            }
            Action::HideDialog => {
                self.clear_dialog();
                Action::None
            }
            _ => action,
        }
    }

    fn render(&mut self, f: &mut Frame, rect: Rect) {
        if let Some(dialog_type) = self.dialog_type.clone() {
            match dialog_type {
                DialogType::TaskCreation { .. } => self.render_task_creation_dialog(f, rect),
                DialogType::TaskEdit { .. } => self.render_task_edit_dialog(f, rect),
                DialogType::ProjectCreation => {
                    self.render_project_creation_dialog(f, rect);
                }
                DialogType::ProjectEdit { .. } => {
                    self.render_project_edit_dialog(f, rect);
                }
                DialogType::LabelCreation => {
                    self.render_label_creation_dialog(f, rect);
                }
                DialogType::LabelEdit { .. } => {
                    self.render_label_edit_dialog(f, rect);
                }
                DialogType::DeleteConfirmation { item_type, .. } => {
                    self.render_delete_confirmation_dialog(f, rect, &item_type);
                }
                DialogType::Info(message) => {
                    self.render_info_dialog(f, rect, &message);
                }
                DialogType::Error(message) => {
                    self.render_error_dialog(f, rect, &message);
                }
                DialogType::Help => {
                    self.render_help_dialog(f, rect);
                }
                DialogType::Logs => {
                    self.render_logs_dialog(f, rect);
                }
                DialogType::TaskSearch => {
                    self.render_task_search_dialog(f, rect);
                }
                DialogType::GoToCompletedDay => {
                    task_dialogs::render_go_to_day_dialog(
                        f,
                        rect,
                        &self.input_buffer,
                        self.cursor_position,
                        &self.theme,
                    );
                }
                DialogType::Reschedule {
                    content, is_recurring, ..
                } => {
                    task_dialogs::render_reschedule_dialog(
                        f,
                        rect,
                        &self.input_buffer,
                        self.cursor_position,
                        &content,
                        is_recurring,
                        &self.theme,
                    );
                }
                DialogType::ProjectFilter { current } => {
                    let view = filter_dialogs::PickerView {
                        title: "Filter by Project",
                        list_title: "Projects",
                        action: " Apply",
                        query: &self.input_buffer,
                        cursor: self.cursor_position,
                        rows: self
                            .filter_entries()
                            .into_iter()
                            .map(|entry| filter_dialogs::PickerRow {
                                label: match entry.project {
                                    Some(_) => format!("#{}", entry.name),
                                    None => entry.name,
                                },
                                depth: entry.depth,
                                current: entry.project == current,
                            })
                            .collect(),
                        selected: self.selected_project_index,
                    };
                    filter_dialogs::render_picker_dialog(f, rect, &view, &self.theme);
                }
                DialogType::ParentPicker {
                    content,
                    current_parent,
                    ..
                } => {
                    let title = format!("Move '{content}' under…");
                    let view = filter_dialogs::PickerView {
                        title: &title,
                        list_title: "Tasks in this project",
                        action: " Move",
                        query: &self.input_buffer,
                        cursor: self.cursor_position,
                        rows: self
                            .parent_entries()
                            .into_iter()
                            .map(|choice| filter_dialogs::PickerRow {
                                current: choice.parent == current_parent,
                                label: choice.label,
                                depth: choice.depth,
                            })
                            .collect(),
                        selected: self.selected_project_index,
                    };
                    filter_dialogs::render_picker_dialog(f, rect, &view, &self.theme);
                }
            }
        }
    }
}
