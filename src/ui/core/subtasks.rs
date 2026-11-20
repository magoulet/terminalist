//! Where subtasks go in a task list.
//!
//! A view only loads the tasks it matches, so a subtask can be in the list while its parent
//! is not: a subtask due today under a parent with no date, for instance. Such a subtask is
//! shown at the top level with its parent's name next to it, instead of disappearing.

use crate::entities::task;
use std::collections::HashSet;
use uuid::Uuid;

/// Whether `task` starts a row of its own: it has no parent, or its parent isn't loaded.
#[must_use]
pub fn is_root(task: &task::Model, loaded: &HashSet<Uuid>) -> bool {
    task.parent_uuid.is_none_or(|parent| !loaded.contains(&parent))
}

/// Parents the loaded subtasks point at that are not loaded themselves, without repeats.
#[must_use]
pub fn missing_parents(tasks: &[task::Model]) -> Vec<Uuid> {
    let loaded: HashSet<Uuid> = tasks.iter().map(|task| task.uuid).collect();
    let mut seen = HashSet::new();
    tasks
        .iter()
        .filter_map(|task| task.parent_uuid)
        .filter(|parent| !loaded.contains(parent) && seen.insert(*parent))
        .collect()
}

/// One row of the "move under" picker. `parent` is `None` for the "No parent" row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentChoice {
    pub parent: Option<Uuid>,
    pub label: String,
    pub depth: usize,
}

/// Every task `moving` and its subtasks hang from, `moving` included. None of them can
/// become its parent without making a loop.
#[must_use]
pub fn self_and_descendants(tasks: &[task::Model], moving: Uuid) -> HashSet<Uuid> {
    let mut members = HashSet::from([moving]);
    loop {
        let before = members.len();
        for task in tasks {
            if task.parent_uuid.is_some_and(|parent| members.contains(&parent)) {
                members.insert(task.uuid);
            }
        }
        if members.len() == before {
            return members;
        }
    }
}

/// The picker rows for moving `moving` under another task of its project: "No parent (top
/// level)" first, then every open task whose title contains `query` (case-insensitive), in
/// tree order with its depth. The task itself and its own subtasks are left out.
#[must_use]
pub fn parent_choices(project_tasks: &[task::Model], moving: Uuid, query: &str) -> Vec<ParentChoice> {
    fn visit(
        node: &task::Model,
        depth: usize,
        tasks: &[&task::Model],
        seen: &mut HashSet<Uuid>,
        out: &mut Vec<(Uuid, String, usize)>,
    ) {
        if !seen.insert(node.uuid) {
            return;
        }
        out.push((node.uuid, node.content.clone(), depth));
        for child in tasks.iter().filter(|t| t.parent_uuid == Some(node.uuid)) {
            visit(child, depth + 1, tasks, seen, out);
        }
    }

    let excluded = self_and_descendants(project_tasks, moving);
    let open: Vec<&task::Model> = project_tasks
        .iter()
        .filter(|task| !task.is_completed && !task.is_deleted && !excluded.contains(&task.uuid))
        .collect();
    let present: HashSet<Uuid> = open.iter().map(|task| task.uuid).collect();

    let mut ordered = Vec::with_capacity(open.len());
    let mut seen = HashSet::new();
    for root in open.iter().filter(|task| is_root(task, &present)) {
        visit(root, 0, &open, &mut seen, &mut ordered);
    }

    let query = query.to_lowercase();
    let mut choices = vec![ParentChoice {
        parent: None,
        label: "No parent (top level)".to_string(),
        depth: 0,
    }];
    choices.extend(
        ordered
            .into_iter()
            .filter(|(_, content, _)| content.to_lowercase().contains(&query))
            .map(|(uuid, label, depth)| ParentChoice {
                parent: Some(uuid),
                label,
                depth,
            }),
    );
    choices
}
