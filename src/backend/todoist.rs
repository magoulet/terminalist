//! Todoist backend implementation.

use super::{
    Backend, BackendError, BackendLabel, BackendProject, BackendSection, BackendTask, CreateLabelArgs,
    CreateProjectArgs, CreateTaskArgs, MoveTarget, UpdateLabelArgs, UpdateProjectArgs, UpdateTaskArgs,
};
use async_trait::async_trait;
use todoist_api::{TodoistError, TodoistWrapper};

const API_BASE: &str = "https://api.todoist.com/api/v1";

fn map_err(e: TodoistError) -> BackendError {
    match e {
        TodoistError::AuthenticationError { .. } | TodoistError::AuthorizationError { .. } => {
            BackendError::Auth(e.to_string())
        }
        TodoistError::NotFound { .. } => BackendError::NotFound(e.to_string()),
        TodoistError::NetworkError { .. }
        | TodoistError::RateLimited { .. }
        | TodoistError::ServerError { .. }
        | TodoistError::EmptyResponse { .. } => BackendError::Network(e.to_string()),
        TodoistError::ParseError { .. } | TodoistError::ValidationError { .. } => {
            BackendError::InvalidData(e.to_string())
        }
        TodoistError::Generic { .. } => BackendError::Other(e.to_string()),
    }
}

/// Sorts a failed HTTP status into the backend error categories, the same way
/// `todoist_api` does for the calls it wraps.
fn status_error(call: &str, status: u16, body: &str) -> BackendError {
    let message = format!("{call} failed with HTTP {status}: {body}");
    match status {
        401 | 403 => BackendError::Auth(message),
        404 => BackendError::NotFound(message),
        429 | 500..=599 => BackendError::Network(message),
        400 | 422 => BackendError::InvalidData(message),
        _ => BackendError::Other(message),
    }
}

/// Todoist backend implementation.
pub struct TodoistBackend {
    wrapper: TodoistWrapper,
    // `todoist_api` has no Quick Add call, and its move call only takes a project, so those
    // two endpoints are called directly.
    http: reqwest::Client,
    api_token: String,
}

impl TodoistBackend {
    /// Create a new Todoist backend with the provided API token.
    pub fn new(api_token: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            wrapper: TodoistWrapper::new(api_token.clone()),
            http,
            api_token,
        }
    }

    /// POSTs `body` to an API v1 endpoint that answers with a task, for the calls
    /// `todoist_api` does not cover. `call` names the call in error messages.
    async fn post_for_task(
        &self,
        call: &str,
        path: &str,
        body: serde_json::Value,
    ) -> Result<BackendTask, BackendError> {
        let response = self
            .http
            .post(format!("{API_BASE}/{path}"))
            .bearer_auth(&self.api_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| BackendError::Network(format!("{call} request failed: {e}")))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| BackendError::Network(format!("{call} response could not be read: {e}")))?;
        if !status.is_success() {
            return Err(status_error(call, status.as_u16(), &text));
        }

        let task: todoist_api::Task = serde_json::from_str(&text)
            .map_err(|e| BackendError::InvalidData(format!("{call} response could not be parsed: {e}")))?;
        Ok(Self::task_to_backend(&task))
    }

    // Helper: Transform Todoist API project → Backend project
    fn project_to_backend(api_project: &todoist_api::Project) -> BackendProject {
        BackendProject {
            remote_id: api_project.id.clone(),
            name: api_project.name.clone(),
            is_favorite: api_project.is_favorite,
            is_inbox: api_project.inbox_project,
            order_index: 0, // order field removed from API v1
            parent_remote_id: api_project.parent_id.clone(),
        }
    }

    // Helper: Transform Todoist API task → Backend task
    fn task_to_backend(api_task: &todoist_api::Task) -> BackendTask {
        let (due_date, due_datetime) = match &api_task.due {
            Some(due) => {
                let (day, time) = crate::utils::datetime::split_todoist_due(&due.date, due.datetime.as_deref());
                (Some(day), time)
            }
            None => (None, None),
        };
        BackendTask {
            remote_id: api_task.id.clone(),
            content: api_task.content.clone(),
            description: Some(api_task.description.clone()),
            project_remote_id: api_task.project_id.clone(),
            section_remote_id: api_task.section_id.clone(),
            parent_remote_id: api_task.parent_id.clone(),
            priority: api_task.priority,
            order_index: 0, // order field removed from API v1
            due_date,
            due_datetime,
            is_recurring: api_task.due.as_ref().map(|d| d.is_recurring).unwrap_or(false),
            deadline: None, // Todoist doesn't have deadline
            duration: api_task.duration.as_ref().map(|d| format!("{} {}", d.amount, d.unit)),
            is_completed: false, // Fetch operations don't include completed tasks
            labels: api_task.labels.clone(),
        }
    }

    // Helper: Transform Todoist API label → Backend label
    fn label_to_backend(api_label: &todoist_api::Label) -> BackendLabel {
        BackendLabel {
            remote_id: api_label.id.clone(),
            name: api_label.name.clone(),
            order_index: api_label.order.unwrap_or(0),
            is_favorite: api_label.is_favorite,
        }
    }

    // Helper: Transform Todoist API section → Backend section
    fn section_to_backend(api_section: &todoist_api::Section) -> BackendSection {
        BackendSection {
            remote_id: api_section.id.clone(),
            name: api_section.name.clone(),
            project_remote_id: api_section.project_id.clone(),
            order_index: api_section.section_order,
        }
    }
}

#[async_trait]
impl Backend for TodoistBackend {
    async fn fetch_projects(&self) -> Result<Vec<BackendProject>, BackendError> {
        let mut all_projects = Vec::new();
        let mut cursor: Option<String> = None;

        // Fetch all pages with limit=200
        loop {
            let response = self.wrapper.get_projects(Some(200), cursor.clone()).await.map_err(map_err)?;

            all_projects.extend(response.results.iter().map(Self::project_to_backend));

            // Check if there are more pages
            if response.next_cursor.is_none() {
                break;
            }
            cursor = response.next_cursor;
        }

        Ok(all_projects)
    }

    async fn fetch_tasks(&self) -> Result<Vec<BackendTask>, BackendError> {
        let mut all_tasks = Vec::new();
        let mut cursor: Option<String> = None;

        // Fetch all pages with limit=200
        loop {
            let response = self.wrapper.get_tasks(Some(200), cursor.clone()).await.map_err(map_err)?;

            all_tasks.extend(response.results.iter().map(Self::task_to_backend));

            // Check if there are more pages
            if response.next_cursor.is_none() {
                break;
            }
            cursor = response.next_cursor;
        }

        Ok(all_tasks)
    }

    async fn fetch_labels(&self) -> Result<Vec<BackendLabel>, BackendError> {
        let mut all_labels = Vec::new();
        let mut cursor: Option<String> = None;

        // Fetch all pages with limit=200
        loop {
            let response = self.wrapper.get_labels(Some(200), cursor.clone()).await.map_err(map_err)?;

            all_labels.extend(response.results.iter().map(Self::label_to_backend));

            // Check if there are more pages
            if response.next_cursor.is_none() {
                break;
            }
            cursor = response.next_cursor;
        }

        Ok(all_labels)
    }

    async fn fetch_sections(&self) -> Result<Vec<BackendSection>, BackendError> {
        let mut all_sections = Vec::new();
        let mut cursor: Option<String> = None;

        // Fetch all pages with limit=200
        loop {
            let response = self.wrapper.get_sections(Some(200), cursor.clone()).await.map_err(map_err)?;

            all_sections.extend(response.results.iter().map(Self::section_to_backend));

            // Check if there are more pages
            if response.next_cursor.is_none() {
                break;
            }
            cursor = response.next_cursor;
        }

        Ok(all_sections)
    }

    async fn create_project(&self, args: CreateProjectArgs) -> Result<BackendProject, BackendError> {
        let todoist_args = todoist_api::CreateProjectArgs {
            name: args.name,
            color: None,
            is_favorite: args.is_favorite,
            parent_id: args.parent_remote_id,
            view_style: None,
        };

        let project = self.wrapper.create_project(&todoist_args).await.map_err(map_err)?;
        Ok(Self::project_to_backend(&project))
    }

    async fn update_project(&self, remote_id: &str, args: UpdateProjectArgs) -> Result<BackendProject, BackendError> {
        let todoist_args = todoist_api::UpdateProjectArgs {
            name: args.name,
            color: None,
            is_favorite: args.is_favorite,
            view_style: None,
        };

        let project = self.wrapper.update_project(remote_id, &todoist_args).await.map_err(map_err)?;
        Ok(Self::project_to_backend(&project))
    }

    async fn delete_project(&self, remote_id: &str) -> Result<(), BackendError> {
        self.wrapper.delete_project(remote_id).await.map_err(map_err)
    }

    async fn create_task(&self, args: CreateTaskArgs) -> Result<BackendTask, BackendError> {
        let todoist_args = todoist_api::CreateTaskArgs {
            content: args.content,
            description: args.description,
            project_id: Some(args.project_remote_id),
            section_id: args.section_remote_id,
            parent_id: args.parent_remote_id,
            priority: args.priority,
            due_date: args.due_date,
            due_datetime: args.due_datetime,
            labels: Some(args.labels),
            duration: args.duration.as_ref().and_then(|d| {
                // CreateTaskArgs.duration is Option<i32> (just the amount)
                let parts: Vec<&str> = d.split_whitespace().collect();
                if !parts.is_empty() {
                    parts[0].parse().ok()
                } else {
                    None
                }
            }),
            ..Default::default()
        };

        let task = self.wrapper.create_task(&todoist_args).await.map_err(map_err)?;
        Ok(Self::task_to_backend(&task))
    }

    async fn quick_add_task(&self, text: &str) -> Result<BackendTask, BackendError> {
        self.post_for_task("Quick Add", "tasks/quick", serde_json::json!({ "text": text }))
            .await
    }

    async fn update_task(&self, remote_id: &str, args: UpdateTaskArgs) -> Result<BackendTask, BackendError> {
        let todoist_args = todoist_api::UpdateTaskArgs {
            content: args.content,
            description: args.description,
            priority: args.priority,
            due_string: args.due_string,
            due_date: args.due_date,
            due_datetime: args.due_datetime,
            labels: args.labels,
            duration: args.duration.as_ref().and_then(|d| {
                // UpdateTaskArgs.duration is Option<i32> (just the amount)
                let parts: Vec<&str> = d.split_whitespace().collect();
                if !parts.is_empty() {
                    parts[0].parse().ok()
                } else {
                    None
                }
            }),
            ..Default::default()
        };

        let task = self.wrapper.update_task(remote_id, &todoist_args).await.map_err(map_err)?;
        Ok(Self::task_to_backend(&task))
    }

    async fn move_task(&self, remote_id: &str, project_remote_id: &str) -> Result<(), BackendError> {
        self.wrapper.move_task(remote_id, project_remote_id).await.map_err(map_err)?;
        Ok(())
    }

    async fn move_task_to(&self, remote_id: &str, target: MoveTarget) -> Result<BackendTask, BackendError> {
        let body = match target {
            MoveTarget::Project(id) => serde_json::json!({ "project_id": id }),
            MoveTarget::Section(id) => serde_json::json!({ "section_id": id }),
            MoveTarget::Parent(id) => serde_json::json!({ "parent_id": id }),
        };
        self.post_for_task("Move", &format!("tasks/{remote_id}/move"), body).await
    }

    async fn delete_task(&self, remote_id: &str) -> Result<(), BackendError> {
        self.wrapper.delete_task(remote_id).await.map_err(map_err)
    }

    async fn complete_task(&self, remote_id: &str) -> Result<(), BackendError> {
        self.wrapper.complete_task(remote_id).await.map_err(map_err)
    }

    async fn reopen_task(&self, remote_id: &str) -> Result<(), BackendError> {
        self.wrapper.reopen_task(remote_id).await.map_err(map_err)
    }

    async fn create_label(&self, args: CreateLabelArgs) -> Result<BackendLabel, BackendError> {
        let todoist_args = todoist_api::CreateLabelArgs {
            name: args.name,
            color: None,
            is_favorite: args.is_favorite,
            ..Default::default()
        };

        let label = self.wrapper.create_label(&todoist_args).await.map_err(map_err)?;
        Ok(Self::label_to_backend(&label))
    }

    async fn update_label(&self, remote_id: &str, args: UpdateLabelArgs) -> Result<BackendLabel, BackendError> {
        let todoist_args = todoist_api::UpdateLabelArgs {
            name: args.name,
            color: None,
            is_favorite: args.is_favorite,
            ..Default::default()
        };

        let label = self.wrapper.update_label(remote_id, &todoist_args).await.map_err(map_err)?;
        Ok(Self::label_to_backend(&label))
    }

    async fn delete_label(&self, remote_id: &str) -> Result<(), BackendError> {
        self.wrapper.delete_label(remote_id).await.map_err(map_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from the Quick Add response in Todoist's OpenAPI spec.
    const QUICK_ADD_BODY: &str = r#"{
            "user_id": "1", "id": "t1", "project_id": "p1", "section_id": null, "parent_id": null,
            "added_by_uid": "1", "assigned_by_uid": null, "responsible_uid": null,
            "labels": ["phone"], "deadline": null, "duration": null, "is_collapsed": false,
            "checked": false, "is_deleted": false, "added_at": "2026-11-18T09:00:00Z",
            "completed_at": null, "completed_by_uid": null, "updated_at": null,
            "due": {"date": "2026-11-19", "string": "tomorrow", "lang": "en", "is_recurring": false},
            "priority": 4, "child_order": 1, "order_key": "a", "content": "Call Bob",
            "description": "", "note_count": 0, "day_order": -1, "completed_count": 0, "postponed_count": 0
        }"#;

    #[test]
    fn quick_add_status_maps_to_backend_error_categories() {
        assert!(matches!(status_error("Call", 401, ""), BackendError::Auth(_)));
        assert!(matches!(status_error("Call", 403, ""), BackendError::Auth(_)));
        assert!(matches!(status_error("Call", 404, ""), BackendError::NotFound(_)));
        assert!(matches!(status_error("Call", 429, ""), BackendError::Network(_)));
        assert!(matches!(status_error("Call", 503, ""), BackendError::Network(_)));
        assert!(matches!(status_error("Call", 400, ""), BackendError::InvalidData(_)));
        assert!(matches!(status_error("Call", 418, ""), BackendError::Other(_)));
    }

    #[test]
    fn quick_add_response_parses_into_a_backend_task() {
        let body = QUICK_ADD_BODY;
        let task: todoist_api::Task = serde_json::from_str(body).expect("Quick Add body parses");
        let backend = TodoistBackend::task_to_backend(&task);
        assert_eq!(backend.content, "Call Bob");
        assert_eq!(backend.project_remote_id, "p1");
        assert_eq!(backend.due_date.as_deref(), Some("2026-11-19"));
        assert_eq!(backend.labels, vec!["phone".to_string()]);
    }

    #[test]
    fn timed_due_date_is_split_into_day_and_time() {
        let mut task: todoist_api::Task = serde_json::from_str(QUICK_ADD_BODY).expect("body parses");
        task.due = Some(todoist_api::Due {
            string: "fri 1pm".to_string(),
            date: "2026-11-20T13:00:00".to_string(),
            is_recurring: false,
            datetime: None,
            timezone: None,
            lang: Some("en".to_string()),
        });
        let backend = TodoistBackend::task_to_backend(&task);
        assert_eq!(backend.due_date.as_deref(), Some("2026-11-20"));
        assert_eq!(backend.due_datetime.as_deref(), Some("2026-11-20T13:00:00"));
    }
}
