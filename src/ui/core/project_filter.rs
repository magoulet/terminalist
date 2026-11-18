//! Narrowing a view to one project and its subprojects.
//!
//! The filter is set with `f` and applies to the Today, Tomorrow, Upcoming and label views.
//! A project view already shows a single project, so the filter is ignored there.

use crate::entities::{project, task};
use crate::ui::core::SidebarSelection;
use std::collections::HashSet;
use uuid::Uuid;

/// Whether the filter applies to this view.
#[must_use]
pub fn applies_to(selection: SidebarSelection) -> bool {
    !matches!(selection, SidebarSelection::Project(_))
}

/// The project and every project nested under it, at any depth.
#[must_use]
pub fn subtree(projects: &[project::Model], root: Uuid) -> HashSet<Uuid> {
    let mut members = HashSet::from([root]);
    // Repeat until no new child turns up, so nesting depth and list order don't matter.
    loop {
        let before = members.len();
        for project in projects {
            if project.parent_uuid.is_some_and(|parent| members.contains(&parent)) {
                members.insert(project.uuid);
            }
        }
        if members.len() == before {
            return members;
        }
    }
}

/// The tasks that belong to `root` or one of its subprojects.
#[must_use]
pub fn filter_tasks(tasks: &[task::Model], projects: &[project::Model], root: Uuid) -> Vec<task::Model> {
    let members = subtree(projects, root);
    tasks
        .iter()
        .filter(|task| members.contains(&task.project_uuid))
        .cloned()
        .collect()
}

/// Projects in tree order with their depth: the inbox first, then favorites, then by name,
/// with children right under their parent. A project whose parent is missing counts as a root.
#[must_use]
pub fn tree_order(projects: &[project::Model]) -> Vec<(&project::Model, usize)> {
    fn sorted(mut level: Vec<&project::Model>) -> Vec<&project::Model> {
        level.sort_by(|a, b| {
            b.is_inbox_project
                .cmp(&a.is_inbox_project)
                .then(b.is_favorite.cmp(&a.is_favorite))
                .then(a.name.cmp(&b.name))
        });
        level
    }

    fn visit<'a>(
        node: &'a project::Model,
        depth: usize,
        projects: &'a [project::Model],
        seen: &mut HashSet<Uuid>,
        out: &mut Vec<(&'a project::Model, usize)>,
    ) {
        // A parent loop in bad data would otherwise recurse forever.
        if !seen.insert(node.uuid) {
            return;
        }
        out.push((node, depth));
        let children = projects.iter().filter(|p| p.parent_uuid == Some(node.uuid)).collect();
        for child in sorted(children) {
            visit(child, depth + 1, projects, seen, out);
        }
    }

    let known: HashSet<Uuid> = projects.iter().map(|p| p.uuid).collect();
    let roots = projects
        .iter()
        .filter(|p| p.parent_uuid.is_none_or(|parent| !known.contains(&parent)))
        .collect();

    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(projects.len());
    for root in sorted(roots) {
        visit(root, 0, projects, &mut seen, &mut out);
    }
    out
}

/// One row of the filter picker. `project` is `None` for the "All projects" row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerEntry {
    pub project: Option<Uuid>,
    pub name: String,
    pub depth: usize,
}

/// The picker rows for a typed query: "All projects" first, then every project whose name
/// contains the query (case-insensitive), in tree order.
#[must_use]
pub fn picker_entries(projects: &[project::Model], query: &str) -> Vec<PickerEntry> {
    let query = query.to_lowercase();
    let mut entries = vec![PickerEntry {
        project: None,
        name: "All projects".to_string(),
        depth: 0,
    }];
    entries.extend(
        tree_order(projects)
            .into_iter()
            .filter(|(project, _)| project.name.to_lowercase().contains(&query))
            .map(|(project, depth)| PickerEntry {
                project: Some(project.uuid),
                name: project.name.clone(),
                depth,
            }),
    );
    entries
}
