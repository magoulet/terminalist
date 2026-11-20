use std::collections::HashSet;
use terminalist::entities::task;
use terminalist::ui::core::subtasks::{is_root, missing_parents};
use uuid::Uuid;

fn task_model(content: &str, parent: Option<Uuid>) -> task::Model {
    task::Model {
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

/// A subtask whose parent is in the view nests under it; one whose parent is not gets its own row.
#[test]
fn subtask_is_a_root_only_when_its_parent_is_not_loaded() {
    let parent = task_model("Plan trip", None);
    let nested = task_model("Book hotel", Some(parent.uuid));
    let stray = task_model("Renew passport", Some(Uuid::new_v4()));
    let loaded: HashSet<Uuid> = [parent.uuid, nested.uuid, stray.uuid].into();

    assert!(is_root(&parent, &loaded));
    assert!(!is_root(&nested, &loaded));
    assert!(is_root(&stray, &loaded));
}

#[test]
fn missing_parents_lists_each_absent_parent_once() {
    let loaded_parent = task_model("Plan trip", None);
    let absent = Uuid::new_v4();
    let tasks = vec![
        loaded_parent.clone(),
        task_model("Book hotel", Some(loaded_parent.uuid)),
        task_model("Renew passport", Some(absent)),
        task_model("Get photos", Some(absent)),
    ];
    assert_eq!(missing_parents(&tasks), vec![absent]);
}

use terminalist::ui::core::subtasks::parent_choices;

/// The task being moved and its own subtasks can't become its parent; finished tasks aren't
/// offered; the rest come in tree order after "No parent".
#[test]
fn parent_choices_leave_out_the_task_its_subtasks_and_finished_tasks() {
    let trip = task_model("Plan trip", None);
    let hotel = task_model("Book hotel", Some(trip.uuid));
    let moving = task_model("Packing", None);
    let under_moving = task_model("Buy adapter", Some(moving.uuid));
    let mut done = task_model("Old errand", None);
    done.is_completed = true;
    let tasks = vec![trip.clone(), hotel.clone(), moving.clone(), under_moving, done];

    let choices = parent_choices(&tasks, moving.uuid, "");
    let rows: Vec<(Option<Uuid>, usize)> = choices.iter().map(|c| (c.parent, c.depth)).collect();
    assert_eq!(rows, vec![(None, 0), (Some(trip.uuid), 0), (Some(hotel.uuid), 1)]);
}

#[test]
fn parent_choices_narrow_by_title_but_keep_the_top_level_row() {
    let trip = task_model("Plan trip", None);
    let hotel = task_model("Book hotel", Some(trip.uuid));
    let moving = task_model("Packing", None);
    let tasks = vec![trip, hotel.clone(), moving.clone()];

    let choices = parent_choices(&tasks, moving.uuid, "HOT");
    assert_eq!(choices.len(), 2);
    assert_eq!(choices[0].parent, None);
    assert_eq!(choices[1].parent, Some(hotel.uuid));
}
