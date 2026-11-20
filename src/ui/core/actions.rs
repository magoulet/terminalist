use crate::sync::SyncStatus;
use crate::ui::core::operations::Due;
use uuid::Uuid;

/// Represents the currently selected item in the sidebar.
///
/// Projects and labels are named by their local UUID rather than by a position in the loaded
/// vectors. A sync that adds, removes or reorders a project would otherwise slide the
/// selection onto a different one without anything noticing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarSelection {
    #[default]
    Today, // Today view (special view)
    Tomorrow, // Tomorrow view (special view)
    Upcoming, // Upcoming view (tasks with future due dates)
    Label(Uuid),
    Project(Uuid),
}

/// Why a data load was scheduled; decides what happens to the cursor when the reload lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadKind {
    /// First load of the session; resolves the configured `default_project`.
    Initial,
    /// User-triggered (navigation, task op, debug refresh). Cursor stays on its row.
    User,
    /// Background sync. Cursor re-anchors to the task it was on.
    Background,
}

#[derive(Debug, Clone)]
pub enum Action {
    // Navigation
    NavigateToSidebar(SidebarSelection),
    /// Show only one project (and its subprojects) in the date and label views. `None` clears it.
    SetProjectFilter(Option<Uuid>),

    // Task operations
    CompleteTask(Uuid),
    DeleteTask(Uuid),
    CyclePriority(Uuid),
    SetTaskDue(Uuid, Due),
    /// Reschedule from free text, parsed by the backend.
    RescheduleTask {
        task_uuid: Uuid,
        due: String,
    },
    CreateTask {
        content: String,
        project_uuid: Option<Uuid>,
        /// `None` when the description field was left empty.
        description: Option<String>,
        /// Set when the new task is a subtask of this task.
        parent_uuid: Option<Uuid>,
    },
    /// Open the "move under" picker for a task.
    OpenParentPicker(Uuid),
    /// Make a task a subtask of `parent`, or move it to the top level with `None`.
    SetTaskParent {
        task_uuid: Uuid,
        parent: Option<Uuid>,
    },
    EditTask {
        task_uuid: Uuid,
        content: String,
        /// Set when the edit also moves the task to another project.
        move_to_project: Option<Uuid>,
        /// Set only when the description changed. `Some("")` clears it.
        description: Option<String>,
    },
    RestoreTask(Uuid),

    // Project operations
    CreateProject {
        name: String,
        parent_uuid: Option<Uuid>,
    },
    EditProject {
        project_uuid: Uuid,
        name: String,
    },
    DeleteProject(Uuid),

    // Label operations
    CreateLabel {
        name: String,
    },
    EditLabel {
        label_uuid: Uuid,
        name: String,
    },
    DeleteLabel(Uuid),

    // Sync operations
    StartSync,
    RefreshLocalData, // Debug mode: refresh from local DB without API sync
    SyncCompleted(SyncStatus),
    SyncFailed(String),
    DataLoaded {
        kind: LoadKind,
        projects: Vec<crate::entities::project::Model>,
        labels: Vec<crate::entities::label::Model>,
        sections: Vec<crate::entities::section::Model>,
        tasks: Vec<crate::entities::task::Model>,
        /// Titles of parents that subtasks in `tasks` point at but that are not loaded.
        parent_names: std::collections::HashMap<Uuid, String>,
    },
    SearchTasks(String), // Query for task search
    SearchResultsLoaded {
        query: String,
        results: Vec<crate::entities::task::Model>,
    },

    // Data refresh after task operations
    RefreshData,

    // UI operations
    ToggleSidebar,
    ShowHelp(bool),
    ShowDialog(DialogType),
    HideDialog,
    HelpScrollUp,
    HelpScrollDown,
    HelpScrollToTop,
    HelpScrollToBottom,

    // App control
    Quit,
    None,
}

#[derive(Debug, Clone)]
pub enum DialogType {
    TaskCreation {
        default_project_uuid: Option<Uuid>,
        /// Set when creating a subtask: the parent's UUID and title.
        parent: Option<(Uuid, String)>,
    },
    TaskEdit {
        task_uuid: Uuid,
        content: String,
        description: String,
        project_uuid: Uuid,
    },
    ProjectCreation,
    ProjectEdit {
        project_uuid: Uuid,
        name: String,
    },
    LabelCreation,
    LabelEdit {
        label_uuid: Uuid,
        name: String,
    },
    DeleteConfirmation {
        item_type: String,
        item_uuid: Uuid,
    },
    Error(String),
    Info(String),
    Help,
    Logs,
    TaskSearch,
    /// Type a new due date for a task.
    Reschedule {
        task_uuid: Uuid,
        content: String,
        /// The task repeats, and a plain date would replace its repeat pattern.
        is_recurring: bool,
    },
    /// Pick the task `task_uuid` moves under, among the open tasks of its project.
    ParentPicker {
        task_uuid: Uuid,
        content: String,
        current_parent: Option<Uuid>,
        candidates: Vec<crate::entities::task::Model>,
    },
    /// Pick the project the date and label views are narrowed to.
    ProjectFilter {
        current: Option<Uuid>,
    },
}
