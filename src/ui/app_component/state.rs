//! The data the app renders, kept apart from the components that draw it.

use crate::entities::{label, project, section, task};
use crate::ui::core::{project_filter, SidebarSelection};
use uuid::Uuid;

/// Application state separate from UI concerns
#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub projects: Vec<project::Model>,
    pub tasks: Vec<task::Model>,
    pub labels: Vec<label::Model>,
    pub sections: Vec<section::Model>,
    pub sidebar_selection: SidebarSelection,
    /// Project the date and label views are narrowed to. It stays set across views.
    pub project_filter: Option<Uuid>,
    /// Titles of parents that loaded subtasks point at but that the view did not load.
    pub parent_names: std::collections::HashMap<Uuid, String>,
    pub loading: bool,
    pub show_help: bool,
    /// didnt we just got rid of custom scrolling ?
    pub help_scroll_offset: usize,
}

impl AppState {
    /// Update all data at once
    pub fn update_data(
        &mut self,
        projects: Vec<project::Model>,
        labels: Vec<label::Model>,
        sections: Vec<section::Model>,
        tasks: Vec<task::Model>,
    ) {
        self.projects = projects;
        self.labels = labels;
        self.sections = sections;
        self.tasks = tasks;
    }

    /// The project filter, if it applies to the current view.
    pub fn active_project_filter(&self) -> Option<Uuid> {
        self.project_filter
            .filter(|_| project_filter::applies_to(self.sidebar_selection))
    }

    /// The loaded tasks after the project filter. `tasks` itself stays unfiltered, so
    /// changing the filter needs no reload.
    pub fn visible_tasks(&self) -> Vec<task::Model> {
        match self.active_project_filter() {
            Some(root) => project_filter::filter_tasks(&self.tasks, &self.projects, root),
            None => self.tasks.clone(),
        }
    }

    /// Name of the project the current view is filtered to, for the task list title.
    pub fn project_filter_name(&self) -> Option<String> {
        let root = self.active_project_filter()?;
        self.projects
            .iter()
            .find(|project| project.uuid == root)
            .map(|project| project.name.clone())
    }

    /// Drops a filter whose project no longer exists, such as one deleted from another client.
    /// Returns whether it did.
    pub fn drop_stale_project_filter(&mut self) -> bool {
        let stale = self
            .project_filter
            .is_some_and(|root| !self.projects.iter().any(|project| project.uuid == root));
        if stale {
            self.project_filter = None;
        }
        stale
    }

    /// Whether the sidebar selection still names something that exists.
    ///
    /// A project or label deleted from another client is simply absent from the next sync,
    /// which leaves the selection pointing at nothing.
    pub fn selection_is_live(&self) -> bool {
        match self.sidebar_selection {
            SidebarSelection::Project(uuid) => self.projects.iter().any(|project| project.uuid == uuid),
            SidebarSelection::Label(uuid) => self.labels.iter().any(|label| label.uuid == uuid),
            SidebarSelection::Today | SidebarSelection::Tomorrow | SidebarSelection::Upcoming => true,
        }
    }
}
