use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use terminalist::entities::project;
use terminalist::ui::components::dialog_component::TaskField;
use terminalist::ui::components::DialogComponent;
use terminalist::ui::core::{
    actions::{Action, DialogType},
    Component,
};
use uuid::Uuid;

fn project_model(uuid: Uuid, name: &str, is_inbox_project: bool) -> project::Model {
    project::Model {
        uuid,
        backend_uuid: Uuid::nil(),
        remote_id: name.to_string(),
        name: name.to_string(),
        is_favorite: false,
        is_inbox_project,
        order_index: 0,
        parent_uuid: None,
    }
}

fn press(dialog: &mut DialogComponent, code: KeyCode) -> Action {
    dialog.handle_key_events(KeyEvent::new(code, KeyModifiers::NONE))
}

fn type_text(dialog: &mut DialogComponent, text: &str) {
    for c in text.chars() {
        press(dialog, KeyCode::Char(c));
    }
}

/// Tab twice moves from the title, past the description, to the project field.
fn focus_project(dialog: &mut DialogComponent) {
    press(dialog, KeyCode::Tab);
    press(dialog, KeyCode::Tab);
    assert_eq!(dialog.task_field, TaskField::Project);
}

/// Opens an edit dialog on a task living in `projects[0]`.
fn editing_dialog(projects: &[project::Model], task_uuid: Uuid) -> DialogComponent {
    let mut dialog = DialogComponent::new();
    dialog.update_data_with_tasks(projects.to_vec(), Vec::new(), Vec::new());
    dialog.update(Action::ShowDialog(DialogType::TaskEdit {
        task_uuid,
        content: "Buy milk".to_string(),
        description: "2% if they have it".to_string(),
        project_uuid: projects[0].uuid,
    }));
    dialog
}

/// Right and Left must cycle the edit dialog's project field through every project, inbox
/// included, and wrap around both ways.
#[test]
fn test_task_edit_arrows_cycle_projects() {
    let inbox = project_model(Uuid::new_v4(), "Inbox", true);
    let work = project_model(Uuid::new_v4(), "Work", false);
    let home = project_model(Uuid::new_v4(), "Home", false);
    let projects = vec![inbox.clone(), work.clone(), home.clone()];

    let mut dialog = editing_dialog(&projects, Uuid::new_v4());
    assert_eq!(dialog.selected_task_project_uuid, Some(inbox.uuid));
    focus_project(&mut dialog);

    press(&mut dialog, KeyCode::Right);
    assert_eq!(dialog.selected_task_project_uuid, Some(work.uuid));

    press(&mut dialog, KeyCode::Right);
    assert_eq!(dialog.selected_task_project_uuid, Some(home.uuid));

    press(&mut dialog, KeyCode::Right);
    assert_eq!(dialog.selected_task_project_uuid, Some(inbox.uuid));

    press(&mut dialog, KeyCode::Left);
    assert_eq!(dialog.selected_task_project_uuid, Some(home.uuid));
}

/// Saving after changing the project asks for the move; saving without it does not.
#[test]
fn test_task_edit_submits_move_only_when_project_changed() {
    let inbox = project_model(Uuid::new_v4(), "Inbox", true);
    let work = project_model(Uuid::new_v4(), "Work", false);
    let projects = vec![inbox.clone(), work.clone()];
    let task_uuid = Uuid::new_v4();

    let mut untouched = editing_dialog(&projects, task_uuid);
    match press(&mut untouched, KeyCode::Enter) {
        Action::EditTask { move_to_project, .. } => assert_eq!(move_to_project, None),
        other => panic!("expected EditTask, got {other:?}"),
    }

    let mut moved = editing_dialog(&projects, task_uuid);
    focus_project(&mut moved);
    press(&mut moved, KeyCode::Right);
    match press(&mut moved, KeyCode::Enter) {
        Action::EditTask { move_to_project, .. } => assert_eq!(move_to_project, Some(work.uuid)),
        other => panic!("expected EditTask, got {other:?}"),
    }
}

/// The description is only sent when it changed, so saving a title edit leaves it alone.
#[test]
fn test_task_edit_sends_description_only_when_changed() {
    let projects = vec![project_model(Uuid::new_v4(), "Inbox", true)];

    let mut untouched = editing_dialog(&projects, Uuid::new_v4());
    assert_eq!(untouched.description_buffer, "2% if they have it");
    match press(&mut untouched, KeyCode::Enter) {
        Action::EditTask { description, .. } => assert_eq!(description, None),
        other => panic!("expected EditTask, got {other:?}"),
    }

    let mut cleared = editing_dialog(&projects, Uuid::new_v4());
    press(&mut cleared, KeyCode::Tab);
    for _ in 0.."2% if they have it".len() {
        press(&mut cleared, KeyCode::Backspace);
    }
    match press(&mut cleared, KeyCode::Enter) {
        Action::EditTask { description, .. } => assert_eq!(description.as_deref(), Some("")),
        other => panic!("expected EditTask, got {other:?}"),
    }
}

/// Typing goes to the focused field, and Alt+Enter breaks a description line instead of saving.
#[test]
fn test_task_creation_takes_a_multi_line_description() {
    let mut dialog = DialogComponent::new();
    dialog.update(Action::ShowDialog(DialogType::TaskCreation {
        default_project_uuid: None,
        parent: None,
    }));
    type_text(&mut dialog, "Call Bob tomorrow");
    press(&mut dialog, KeyCode::Tab);
    type_text(&mut dialog, "Ask about");
    let action = dialog.handle_key_events(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
    assert!(matches!(action, Action::None));
    type_text(&mut dialog, "the invoice");

    match press(&mut dialog, KeyCode::Enter) {
        Action::CreateTask {
            content, description, ..
        } => {
            assert_eq!(content, "Call Bob tomorrow");
            assert_eq!(description.as_deref(), Some("Ask about\nthe invoice"));
        }
        other => panic!("expected CreateTask, got {other:?}"),
    }
}

/// An empty description field creates the task without one.
#[test]
fn test_task_creation_without_description_sends_none() {
    let mut dialog = DialogComponent::new();
    dialog.update(Action::ShowDialog(DialogType::TaskCreation {
        default_project_uuid: None,
        parent: None,
    }));
    type_text(&mut dialog, "Water plants");
    match press(&mut dialog, KeyCode::Enter) {
        Action::CreateTask { description, .. } => assert_eq!(description, None),
        other => panic!("expected CreateTask, got {other:?}"),
    }
}

fn reschedule_dialog() -> DialogComponent {
    let mut dialog = DialogComponent::new();
    dialog.update(Action::ShowDialog(DialogType::Reschedule {
        task_uuid: Uuid::nil(),
        content: "Pay rent".to_string(),
        is_recurring: false,
    }));
    dialog
}

/// The reschedule dialog sends the typed text, trimmed, for Todoist to parse.
#[test]
fn test_reschedule_submits_the_typed_date() {
    let mut dialog = reschedule_dialog();
    type_text(&mut dialog, " next fri 3pm ");
    match press(&mut dialog, KeyCode::Enter) {
        Action::RescheduleTask { task_uuid, due } => {
            assert_eq!(task_uuid, Uuid::nil());
            assert_eq!(due, "next fri 3pm");
        }
        other => panic!("expected RescheduleTask, got {other:?}"),
    }
    assert!(!dialog.is_visible());
}

/// Enter on an empty field does nothing and keeps the dialog open.
#[test]
fn test_reschedule_ignores_an_empty_date() {
    let mut dialog = reschedule_dialog();
    type_text(&mut dialog, "   ");
    assert!(matches!(press(&mut dialog, KeyCode::Enter), Action::None));
    assert!(dialog.is_visible());
}

/// A subtask dialog sends the parent along and keeps the project fixed.
#[test]
fn test_subtask_creation_sends_the_parent_and_locks_the_project() {
    let work = project_model(Uuid::new_v4(), "Work", false);
    let home = project_model(Uuid::new_v4(), "Home", false);
    let parent = Uuid::new_v4();
    let mut dialog = DialogComponent::new();
    dialog.update_data_with_tasks(vec![work.clone(), home], Vec::new(), Vec::new());
    dialog.update(Action::ShowDialog(DialogType::TaskCreation {
        default_project_uuid: Some(work.uuid),
        parent: Some((parent, "Plan trip".to_string())),
    }));
    type_text(&mut dialog, "Book hotel");
    focus_project(&mut dialog);
    press(&mut dialog, KeyCode::Right);
    assert_eq!(dialog.selected_task_project_uuid, Some(work.uuid));

    match press(&mut dialog, KeyCode::Enter) {
        Action::CreateTask {
            content, parent_uuid, ..
        } => {
            assert_eq!(content, "Book hotel");
            assert_eq!(parent_uuid, Some(parent));
        }
        other => panic!("expected CreateTask, got {other:?}"),
    }
}

fn task_model(content: &str, parent: Option<Uuid>) -> terminalist::entities::task::Model {
    terminalist::entities::task::Model {
        uuid: Uuid::new_v4(),
        backend_uuid: Uuid::nil(),
        remote_id: content.to_string(),
        content: content.to_string(),
        description: None,
        project_uuid: Uuid::nil(),
        section_uuid: None,
        parent_uuid: parent,
        priority: 1,
        order_index: 0,
        due_date: None,
        due_datetime: None,
        is_recurring: false,
        deadline: None,
        duration: None,
        is_completed: false,
        is_deleted: false,
    }
}

fn parent_picker(
    moving: &terminalist::entities::task::Model,
    others: &[terminalist::entities::task::Model],
) -> DialogComponent {
    let mut candidates = others.to_vec();
    candidates.push(moving.clone());
    let mut dialog = DialogComponent::new();
    dialog.update(Action::ShowDialog(DialogType::ParentPicker {
        task_uuid: moving.uuid,
        content: moving.content.clone(),
        current_parent: moving.parent_uuid,
        candidates,
    }));
    dialog
}

/// Typing narrows to a task and Enter moves the task under it.
#[test]
fn test_parent_picker_moves_under_the_chosen_task() {
    let trip = task_model("Plan trip", None);
    let groceries = task_model("Groceries", None);
    let moving = task_model("Book hotel", None);
    let mut dialog = parent_picker(&moving, &[trip.clone(), groceries]);

    type_text(&mut dialog, "trip");
    match press(&mut dialog, KeyCode::Enter) {
        Action::SetTaskParent { task_uuid, parent } => {
            assert_eq!(task_uuid, moving.uuid);
            assert_eq!(parent, Some(trip.uuid));
        }
        other => panic!("expected SetTaskParent, got {other:?}"),
    }
}

/// The picker opens on the current parent; the "No parent" row moves a subtask back out,
/// and picking the parent it already has just closes the dialog.
#[test]
fn test_parent_picker_top_level_row_and_unchanged_parent() {
    let trip = task_model("Plan trip", None);
    let moving = task_model("Book hotel", Some(trip.uuid));

    let mut unchanged = parent_picker(&moving, std::slice::from_ref(&trip));
    assert!(matches!(press(&mut unchanged, KeyCode::Enter), Action::HideDialog));

    let mut out = parent_picker(&moving, &[trip]);
    press(&mut out, KeyCode::Up);
    match press(&mut out, KeyCode::Enter) {
        Action::SetTaskParent { parent, .. } => assert_eq!(parent, None),
        other => panic!("expected SetTaskParent, got {other:?}"),
    }
}

/// The go-to-day box opens the Completed view on the typed day, or explains what it accepts.
#[test]
fn test_go_to_completed_day_reads_the_typed_day() {
    let mut dialog = DialogComponent::new();
    dialog.update(Action::ShowDialog(DialogType::GoToCompletedDay));
    type_text(&mut dialog, "yesterday");
    let yesterday = chrono::Local::now().date_naive() - chrono::Duration::days(1);
    assert!(matches!(press(&mut dialog, KeyCode::Enter), Action::ShowCompletedDay(day) if day == yesterday));

    dialog.update(Action::ShowDialog(DialogType::GoToCompletedDay));
    type_text(&mut dialog, "someday");
    assert!(matches!(
        press(&mut dialog, KeyCode::Enter),
        Action::ShowDialog(DialogType::Error(message)) if message.contains("someday")
    ));
}
