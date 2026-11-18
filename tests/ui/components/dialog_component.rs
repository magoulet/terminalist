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
    }));
    type_text(&mut dialog, "Water plants");
    match press(&mut dialog, KeyCode::Enter) {
        Action::CreateTask { description, .. } => assert_eq!(description, None),
        other => panic!("expected CreateTask, got {other:?}"),
    }
}
