use crate::backend::MoveTarget;
use crate::entities::task;
use crate::repositories::{ProjectRepository, SectionRepository, TaskRepository};
use crate::sync::SyncService;
use crate::utils::datetime;
use anyhow::Result;
use sea_orm::{ActiveValue, EntityTrait, IntoActiveModel, TransactionTrait};
use uuid::Uuid;

/// A task completed on the day the Completed view shows. It is fetched from the backend and
/// never stored: the cache only holds active tasks.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletedTask {
    pub task: task::Model,
    /// When it was completed (RFC 3339).
    pub completed_at: Option<String>,
}

/// What a Quick Add produced, for the confirmation message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickAddOutcome {
    /// The task title after the backend stripped the parsed date, project and labels.
    pub content: String,
    /// Whether the task ended up in the inbox.
    pub in_inbox: bool,
}

/// Decides where a Quick Add task still has to move. Quick Add puts a task in the inbox
/// unless its text names a project, so the dialog's project only applies to an inbox
/// result. Returns `None` when the task is already where it belongs.
fn quick_add_move_target<'a>(landed: &str, inbox: Option<&str>, target: Option<&'a str>) -> Option<&'a str> {
    let target = target?;
    (Some(landed) == inbox && landed != target).then_some(target)
}

impl SyncService {
    /// Retrieves all tasks for a specific project from local storage.
    ///
    /// # Arguments
    /// * `project_id` - The unique identifier of the project
    ///
    /// # Returns
    /// A vector of `task::Model` objects for the specified project
    ///
    /// # Errors
    /// Returns an error if local storage access fails
    pub async fn get_tasks_for_project(&self, project_id: &Uuid) -> Result<Vec<task::Model>> {
        let storage = self.storage.lock().await;
        TaskRepository::get_for_project(&storage.conn, project_id).await
    }

    /// Searches for tasks by content using database-level filtering.
    ///
    /// This method performs fast text search across task content using SQL LIKE queries.
    /// The search is case-insensitive and matches partial content.
    ///
    /// # Arguments
    /// * `query` - The search term to look for in task content
    ///
    /// # Returns
    /// A vector of `task::Model` objects matching the search criteria
    ///
    /// # Errors
    /// Returns an error if local storage access fails
    pub async fn search_tasks(&self, query: &str) -> Result<Vec<task::Model>> {
        let storage = self.storage.lock().await;
        TaskRepository::search(&storage.conn, query).await
    }

    /// Get tasks with a specific label from local storage (fast)
    pub async fn get_tasks_with_label(&self, label_id: Uuid) -> Result<Vec<task::Model>> {
        let storage = self.storage.lock().await;
        TaskRepository::get_with_label(&storage.conn, label_id).await
    }

    /// Retrieves tasks for the "Today" view with business logic.
    ///
    /// This method implements the UI business logic for the Today view by combining
    /// overdue tasks with tasks due today. Overdue tasks are shown first, followed
    /// by today's tasks.
    ///
    /// # Returns
    /// A vector of `task::Model` objects for the Today view, with overdue tasks first
    ///
    /// # Errors
    /// Returns an error if local storage access fails
    pub async fn get_tasks_for_today(&self) -> Result<Vec<task::Model>> {
        let storage = self.storage.lock().await;
        let today = datetime::format_today();
        TaskRepository::get_for_today(&storage.conn, &today).await
    }

    /// Retrieves tasks scheduled for tomorrow.
    ///
    /// This method returns only tasks that are specifically due tomorrow,
    /// without any additional business logic.
    ///
    /// # Returns
    /// A vector of `task::Model` objects due tomorrow
    ///
    /// # Errors
    /// Returns an error if local storage access fails
    pub async fn get_tasks_for_tomorrow(&self) -> Result<Vec<task::Model>> {
        let storage = self.storage.lock().await;
        let tomorrow = datetime::format_date_with_offset(1);
        TaskRepository::get_for_tomorrow(&storage.conn, &tomorrow).await
    }

    /// Retrieves tasks for the "Upcoming" view with business logic.
    ///
    /// This method implements the UI business logic for the Upcoming view by combining
    /// overdue tasks, today's tasks, and tasks due within the next 3 months.
    /// Tasks are ordered as: overdue → today → future (next 3 months).
    ///
    /// # Returns
    /// A vector of `task::Model` objects for the Upcoming view, properly ordered
    ///
    /// # Errors
    /// Returns an error if local storage access fails
    pub async fn get_tasks_for_upcoming(&self) -> Result<Vec<task::Model>> {
        let storage = self.storage.lock().await;
        let today = datetime::format_today();
        let three_months_later = datetime::format_date_with_offset(90);
        TaskRepository::get_for_upcoming(&storage.conn, &today, &three_months_later).await
    }

    /// Titles of the given tasks, keyed by UUID. Used to name the parent of a subtask whose
    /// parent is not in the current view. Unknown UUIDs are skipped.
    pub async fn task_titles(&self, uuids: &[Uuid]) -> Result<std::collections::HashMap<Uuid, String>> {
        let storage = self.storage.lock().await;
        let mut titles = std::collections::HashMap::new();
        for uuid in uuids {
            if let Some(task) = TaskRepository::get_by_id(&storage.conn, uuid).await? {
                titles.insert(*uuid, task.content);
            }
        }
        Ok(titles)
    }

    /// Tasks completed on `day` (a local calendar day), most recent first. They come straight
    /// from the backend. Each gets the UUID of its cached copy when there is one, and project,
    /// section and parent resolve against the cache and the rest of the day's list. Tasks in a
    /// project the cache doesn't know are left out.
    ///
    /// # Errors
    /// Returns an error if the backend call or a cache lookup fails
    pub async fn completed_on(&self, day: chrono::NaiveDate) -> Result<Vec<CompletedTask>> {
        let (since, until) = datetime::local_day_bounds_utc(day);
        let mut fetched = self.get_backend().await?.fetch_completed_tasks(&since, &until).await?;
        fetched.sort_by(|a, b| b.completed_at.cmp(&a.completed_at));

        let storage = self.storage.lock().await;
        let mut uuids = std::collections::HashMap::new();
        for backend_task in &fetched {
            let uuid = TaskRepository::get_by_remote_id(&storage.conn, &self.backend_uuid, &backend_task.remote_id)
                .await?
                .map_or_else(Uuid::new_v4, |cached| cached.uuid);
            uuids.insert(backend_task.remote_id.clone(), uuid);
        }

        let mut completed = Vec::with_capacity(fetched.len());
        for backend_task in fetched {
            let Ok(project_uuid) = Self::lookup_project_uuid(
                &storage.conn,
                &self.backend_uuid,
                &backend_task.project_remote_id,
                "completed tasks",
            )
            .await
            else {
                continue;
            };
            let section_uuid = Self::lookup_section_uuid(
                &storage.conn,
                &self.backend_uuid,
                backend_task.section_remote_id.as_ref(),
            )
            .await?;
            let parent_uuid = match &backend_task.parent_remote_id {
                Some(remote) => match uuids.get(remote) {
                    Some(uuid) => Some(*uuid),
                    None => TaskRepository::get_by_remote_id(&storage.conn, &self.backend_uuid, remote)
                        .await?
                        .map(|parent| parent.uuid),
                },
                None => None,
            };
            completed.push(CompletedTask {
                task: task::Model {
                    uuid: uuids[&backend_task.remote_id],
                    backend_uuid: self.backend_uuid,
                    remote_id: backend_task.remote_id,
                    content: backend_task.content,
                    description: backend_task.description,
                    project_uuid,
                    section_uuid,
                    parent_uuid,
                    priority: backend_task.priority,
                    order_index: backend_task.order_index,
                    due_date: backend_task.due_date,
                    due_datetime: backend_task.due_datetime,
                    is_recurring: backend_task.is_recurring,
                    deadline: backend_task.deadline,
                    duration: backend_task.duration,
                    is_completed: true,
                    is_deleted: false,
                },
                completed_at: backend_task.completed_at,
            });
        }
        Ok(completed)
    }

    /// Reopens a completed task that may not be in the cache, such as one from the Completed
    /// view. A sync afterwards brings it back as an active task.
    ///
    /// # Errors
    /// Returns an error if the backend call fails
    pub async fn reopen_remote_task(&self, remote_id: &str) -> Result<()> {
        self.get_backend().await?.reopen_task(remote_id).await?;
        Ok(())
    }

    /// Get a single task by ID from local storage (fast)
    pub async fn get_task_by_id(&self, task_id: &Uuid) -> Result<Option<task::Model>> {
        let storage = self.storage.lock().await;
        TaskRepository::get_by_id(&storage.conn, task_id).await
    }

    /// Creates a new task via the remote backend and stores it locally.
    ///
    /// This method creates a task remotely and immediately stores it in local storage
    /// for instant UI updates. The task will be available in the UI without requiring
    /// a full sync operation.
    ///
    /// # Arguments
    /// * `content` - The content/description of the new task
    /// * `project_uuid` - Optional local project UUID to assign the task to a specific project
    ///
    /// # Errors
    /// Returns an error if the backend call fails or local storage update fails
    pub async fn create_task(&self, content: &str, project_uuid: Option<Uuid>) -> Result<()> {
        // Look up remote_id for project if provided
        let remote_project_id = {
            let storage = self.storage.lock().await;
            if let Some(uuid) = project_uuid {
                Some(ProjectRepository::get_remote_id(&storage.conn, &uuid).await?)
            } else {
                None
            }
            // Lock is automatically dropped here when storage goes out of scope
        };

        // Create task via backend using backend CreateTaskArgs (lock is not held)
        let task_args = crate::backend::CreateTaskArgs {
            content: content.to_string(),
            description: None,
            project_remote_id: remote_project_id.unwrap_or_default(),
            section_remote_id: None,
            parent_remote_id: None,
            priority: None,
            due_date: None,
            due_datetime: None,
            duration: None,
            labels: Vec::new(),
        };
        let backend_task = self.get_backend().await?.create_task(task_args).await?;

        // Store the created task in local database immediately for UI refresh
        let storage = self.storage.lock().await;
        let txn = storage.conn.begin().await?;

        // Look up local project UUID from remote project_id
        let project_uuid = Self::lookup_project_uuid(
            &txn,
            &self.backend_uuid,
            &backend_task.project_remote_id,
            "task creation",
        )
        .await?;

        // Look up local section UUID from remote section_id if present
        let section_uuid =
            Self::lookup_section_uuid(&txn, &self.backend_uuid, backend_task.section_remote_id.as_ref()).await?;

        // Look up local parent UUID from remote parent_id if present
        let parent_uuid = if let Some(remote_parent_id) = &backend_task.parent_remote_id {
            TaskRepository::get_by_remote_id(&txn, &self.backend_uuid, remote_parent_id)
                .await?
                .map(|t| t.uuid)
        } else {
            None
        };

        let local_task = task::ActiveModel {
            uuid: ActiveValue::Set(Uuid::new_v4()),
            backend_uuid: ActiveValue::Set(self.backend_uuid),
            remote_id: ActiveValue::Set(backend_task.remote_id),
            content: ActiveValue::Set(backend_task.content),
            description: ActiveValue::Set(backend_task.description),
            project_uuid: ActiveValue::Set(project_uuid),
            section_uuid: ActiveValue::Set(section_uuid),
            parent_uuid: ActiveValue::Set(parent_uuid),
            priority: ActiveValue::Set(backend_task.priority),
            order_index: ActiveValue::Set(backend_task.order_index),
            due_date: ActiveValue::Set(backend_task.due_date),
            due_datetime: ActiveValue::Set(backend_task.due_datetime),
            is_recurring: ActiveValue::Set(backend_task.is_recurring),
            deadline: ActiveValue::Set(backend_task.deadline),
            duration: ActiveValue::Set(backend_task.duration),
            is_completed: ActiveValue::Set(backend_task.is_completed),
            is_deleted: ActiveValue::Set(false),
        };

        use sea_orm::sea_query::OnConflict;
        let mut insert = task::Entity::insert(local_task);
        insert = insert.on_conflict(
            OnConflict::columns([task::Column::BackendUuid, task::Column::RemoteId])
                .update_columns([
                    task::Column::Content,
                    task::Column::Description,
                    task::Column::ProjectUuid,
                    task::Column::SectionUuid,
                    task::Column::ParentUuid,
                    task::Column::Priority,
                    task::Column::OrderIndex,
                    task::Column::DueDate,
                    task::Column::DueDatetime,
                    task::Column::IsRecurring,
                    task::Column::Deadline,
                    task::Column::Duration,
                    task::Column::IsCompleted,
                    task::Column::IsDeleted,
                ])
                .to_owned(),
        );
        insert.exec(&txn).await?;

        txn.commit().await?;

        Ok(())
    }

    /// Creates a task from free text through the backend's natural-language parser
    /// (Todoist Quick Add), so "Call Bob tomorrow 3pm #Work @phone p1" sets the due date,
    /// project, label and priority.
    ///
    /// Quick Add has no project argument. When the text names no project the task lands in
    /// the inbox, and is then moved to `project_uuid` if one was given. A project named in
    /// the text always wins over `project_uuid`.
    ///
    /// A `description` replaces any ` // description` Quick Add parsed out of the text.
    ///
    /// With a `parent_uuid` the task becomes that task's subtask, which also puts it in the
    /// parent's project and section, whatever the text or `project_uuid` say.
    ///
    /// # Errors
    /// Returns an error if the backend call fails or local storage update fails
    pub async fn quick_add_task(
        &self,
        text: &str,
        project_uuid: Option<Uuid>,
        description: Option<&str>,
        parent_uuid: Option<Uuid>,
    ) -> Result<QuickAddOutcome> {
        let backend = self.get_backend().await?;
        let mut backend_task = backend.quick_add_task(text).await?;

        if let Some(parent) = parent_uuid {
            let parent_remote_id = self.get_task_remote_id(&parent).await?;
            let moved = backend
                .move_task_to(&backend_task.remote_id, MoveTarget::Parent(parent_remote_id))
                .await?;
            backend_task.project_remote_id = moved.project_remote_id;
            backend_task.section_remote_id = moved.section_remote_id;
            backend_task.parent_remote_id = moved.parent_remote_id;
        }
        // A subtask's project comes from its parent, so the dialog's project no longer applies.
        let project_uuid = project_uuid.filter(|_| parent_uuid.is_none());

        let (target_remote_id, inbox_remote_id) = {
            let storage = self.storage.lock().await;
            let target = match project_uuid {
                Some(uuid) => Some(ProjectRepository::get_remote_id(&storage.conn, &uuid).await?),
                None => None,
            };
            let inbox = ProjectRepository::get_all(&storage.conn)
                .await?
                .into_iter()
                .find(|project| project.is_inbox_project && project.backend_uuid == self.backend_uuid)
                .map(|project| project.remote_id);
            (target, inbox)
        };

        if let Some(target) = quick_add_move_target(
            &backend_task.project_remote_id,
            inbox_remote_id.as_deref(),
            target_remote_id.as_deref(),
        ) {
            backend.move_task(&backend_task.remote_id, target).await?;
            backend_task.project_remote_id = target.to_string();
            backend_task.section_remote_id = None;
        }

        // Quick Add only takes a one-line description, so a typed one is set separately.
        if let Some(description) = description {
            let args = crate::backend::UpdateTaskArgs {
                content: None,
                description: Some(description.to_string()),
                priority: None,
                due_string: None,
                due_date: None,
                due_datetime: None,
                duration: None,
                labels: None,
            };
            backend.update_task(&backend_task.remote_id, args).await?;
            backend_task.description = Some(description.to_string());
        }

        let in_inbox = inbox_remote_id.as_deref() == Some(backend_task.project_remote_id.as_str());
        let content = backend_task.content.clone();

        // The batch writer also links labels and parents, which a plain insert would not.
        let storage = self.storage.lock().await;
        let txn = storage.conn.begin().await?;
        self.store_tasks_batch(&txn, std::slice::from_ref(&backend_task)).await?;
        txn.commit().await?;

        Ok(QuickAddOutcome { content, in_inbox })
    }

    /// Makes a task a subtask of `parent`, or moves it back to the top level with `None`.
    /// At the top level it stays in its section, if it had one.
    ///
    /// # Errors
    /// Returns an error if the backend call fails or local storage update fails
    pub async fn set_task_parent(&self, task_uuid: &Uuid, parent: Option<Uuid>) -> Result<()> {
        let (remote_id, target) = {
            let storage = self.storage.lock().await;
            let task = TaskRepository::get_by_id(&storage.conn, task_uuid)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Task not found in local storage: {task_uuid}"))?;
            let target = match parent {
                Some(parent) => MoveTarget::Parent(TaskRepository::get_remote_id(&storage.conn, &parent).await?),
                None => match task.section_uuid {
                    Some(section) => match SectionRepository::get_remote_id(&storage.conn, &section).await? {
                        Some(remote) => MoveTarget::Section(remote),
                        None => MoveTarget::Project(
                            ProjectRepository::get_remote_id(&storage.conn, &task.project_uuid).await?,
                        ),
                    },
                    None => {
                        MoveTarget::Project(ProjectRepository::get_remote_id(&storage.conn, &task.project_uuid).await?)
                    }
                },
            };
            (task.remote_id, target)
        };

        let moved = self.get_backend().await?.move_task_to(&remote_id, target).await?;

        // The batch writer resolves the parent, project and section from the moved task.
        let storage = self.storage.lock().await;
        let txn = storage.conn.begin().await?;
        self.store_tasks_batch(&txn, std::slice::from_ref(&moved)).await?;
        txn.commit().await?;
        Ok(())
    }

    /// Update task content, and the description when one is given. `Some("")` clears it.
    pub async fn update_task_content(&self, task_uuid: &Uuid, content: &str, description: Option<&str>) -> Result<()> {
        // Look up the task's remote_id for backend call
        let remote_id = self.get_task_remote_id(task_uuid).await?;

        // Update task via backend using the UpdateTaskArgs structure
        let task_args = crate::backend::UpdateTaskArgs {
            content: Some(content.to_string()),
            description: description.map(str::to_string),
            priority: None,
            due_string: None,
            due_date: None,
            due_datetime: None,
            duration: None,
            labels: None,
        };
        let _task = self.get_backend().await?.update_task(&remote_id, task_args).await?;

        // Update local storage immediately after successful backend call
        let storage = self.storage.lock().await;

        if let Some(task) = TaskRepository::get_by_id(&storage.conn, task_uuid).await? {
            let mut active_model: task::ActiveModel = task.into_active_model();
            active_model.content = ActiveValue::Set(content.to_string());
            if let Some(description) = description {
                active_model.description = ActiveValue::Set(Some(description.to_string()));
            }
            TaskRepository::update(&storage.conn, active_model).await?;
        }

        Ok(())
    }

    /// Move a task to another project. The section is cleared: a section belongs
    /// to one project, so it cannot survive the move.
    pub async fn move_task(&self, task_uuid: &Uuid, project_uuid: &Uuid) -> Result<()> {
        let remote_id = self.get_task_remote_id(task_uuid).await?;
        let remote_project_id = self.get_project_remote_id(project_uuid).await?;

        self.get_backend().await?.move_task(&remote_id, &remote_project_id).await?;

        // Then update local storage
        let storage = self.storage.lock().await;

        if let Some(task) = TaskRepository::get_by_id(&storage.conn, task_uuid).await? {
            let mut active_model: task::ActiveModel = task.into_active_model();
            active_model.project_uuid = ActiveValue::Set(*project_uuid);
            active_model.section_uuid = ActiveValue::Set(None);
            TaskRepository::update(&storage.conn, active_model).await?;
        }

        Ok(())
    }

    /// Update task due date
    pub async fn update_task_due_date(&self, task_uuid: &Uuid, due_date: Option<&str>) -> Result<()> {
        // Look up the task's remote_id for backend call
        let remote_id = self.get_task_remote_id(task_uuid).await?;

        // Update task via backend using the UpdateTaskArgs structure
        let task_args = crate::backend::UpdateTaskArgs {
            content: None,
            description: None,
            priority: None,
            due_string: None,
            due_date: due_date.map(std::string::ToString::to_string),
            due_datetime: None,
            duration: None,
            labels: None,
        };
        let _task = self.get_backend().await?.update_task(&remote_id, task_args).await?;

        // Then update local storage
        let storage = self.storage.lock().await;

        if let Some(task) = TaskRepository::get_by_id(&storage.conn, task_uuid).await? {
            let mut active_model: task::ActiveModel = task.into_active_model();
            active_model.due_date = ActiveValue::Set(due_date.map(|s| s.to_string()));
            TaskRepository::update(&storage.conn, active_model).await?;
        }

        Ok(())
    }

    /// Reschedules a task from free text the backend parses, such as "fri 3pm", "in 2 weeks",
    /// "every monday" or "no date". The cache takes the due date the backend worked out, so
    /// the views sort the task correctly before the next sync.
    ///
    /// Returns the new due date, or `None` when the text removed it.
    ///
    /// # Errors
    /// Returns an error if the backend call fails, for example on text it cannot parse, or
    /// if the local storage update fails
    pub async fn reschedule_task(&self, task_uuid: &Uuid, due: &str) -> Result<Option<String>> {
        let remote_id = self.get_task_remote_id(task_uuid).await?;
        let task_args = crate::backend::UpdateTaskArgs {
            content: None,
            description: None,
            priority: None,
            due_string: Some(due.to_string()),
            due_date: None,
            due_datetime: None,
            duration: None,
            labels: None,
        };
        let updated = self.get_backend().await?.update_task(&remote_id, task_args).await?;

        let storage = self.storage.lock().await;
        if let Some(task) = TaskRepository::get_by_id(&storage.conn, task_uuid).await? {
            let mut active_model: task::ActiveModel = task.into_active_model();
            active_model.due_date = ActiveValue::Set(updated.due_date.clone());
            active_model.due_datetime = ActiveValue::Set(updated.due_datetime.clone());
            active_model.is_recurring = ActiveValue::Set(updated.is_recurring);
            TaskRepository::update(&storage.conn, active_model).await?;
        }

        Ok(updated.due_date)
    }

    /// Update task priority
    pub async fn update_task_priority(&self, task_uuid: &Uuid, priority: i32) -> Result<()> {
        // Look up the task's remote_id for backend call
        let remote_id = self.get_task_remote_id(task_uuid).await?;

        // Update task via backend using the UpdateTaskArgs structure
        let task_args = crate::backend::UpdateTaskArgs {
            content: None,
            description: None,
            priority: Some(priority),
            due_string: None,
            due_date: None,
            due_datetime: None,
            duration: None,
            labels: None,
        };
        let _task = self.get_backend().await?.update_task(&remote_id, task_args).await?;

        // Then update local storage
        let storage = self.storage.lock().await;

        if let Some(task) = TaskRepository::get_by_id(&storage.conn, task_uuid).await? {
            let mut active_model: task::ActiveModel = task.into_active_model();
            active_model.priority = ActiveValue::Set(priority);
            TaskRepository::update(&storage.conn, active_model).await?;
        }

        Ok(())
    }

    /// Marks a task as completed via the remote backend and removes it from local storage.
    ///
    /// This method completes the task remotely (which automatically handles subtasks)
    /// and removes it from local storage since completed tasks are not displayed in the UI.
    /// Subtasks are automatically deleted via database CASCADE constraints.
    ///
    /// # Arguments
    /// * `task_uuid` - The local UUID of the task to complete
    ///
    /// # Errors
    /// Returns an error if the backend call fails or local storage update fails
    pub async fn complete_task(&self, task_uuid: &Uuid) -> Result<()> {
        // Look up the task's remote_id for backend call
        let remote_id = self.get_task_remote_id(task_uuid).await?;

        // Complete the task via backend using remote_id (this handles subtasks automatically)
        self.get_backend().await?.complete_task(&remote_id).await?;

        // Then mark as completed in local storage (soft completion)
        let storage = self.storage.lock().await;

        if let Some(task) = TaskRepository::get_by_id(&storage.conn, task_uuid).await? {
            let mut active_model: task::ActiveModel = task.into_active_model();
            active_model.is_completed = ActiveValue::Set(true);
            TaskRepository::update(&storage.conn, active_model).await?;
        }

        Ok(())
    }

    /// Permanently deletes a task via the remote backend and removes it from local storage.
    ///
    /// This method performs a hard delete of the task remotely, soft delete locally.
    /// The task will be permanently removed and cannot be recovered.
    ///
    /// # Arguments
    /// * `task_uuid` - The local UUID of the task to delete
    ///
    /// # Errors
    /// Returns an error if the backend call fails or local storage update fails
    pub async fn delete_task(&self, task_uuid: &Uuid) -> Result<()> {
        // Look up the task's remote_id for backend call
        let remote_id = self.get_task_remote_id(task_uuid).await?;

        // Delete the task via backend using remote_id
        self.get_backend().await?.delete_task(&remote_id).await?;

        // Then mark as deleted in local storage (soft deletion)
        let storage = self.storage.lock().await;

        if let Some(task) = TaskRepository::get_by_id(&storage.conn, task_uuid).await? {
            let mut active_model: task::ActiveModel = task.into_active_model();
            active_model.is_deleted = ActiveValue::Set(true);
            TaskRepository::update(&storage.conn, active_model).await?;
        }

        Ok(())
    }

    /// Restore a soft-deleted or completed task via the remote backend and locally
    /// For completed tasks, reopens them. For deleted tasks, recreates them via backend.
    pub async fn restore_task(&self, task_id: &Uuid) -> Result<()> {
        // First, get the task from local storage to check its state
        let storage = self.storage.lock().await;
        let task = TaskRepository::get_by_id(&storage.conn, task_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Task not found in local storage: {}", task_id))?;

        if task.is_deleted {
            // For deleted tasks, we need to recreate them via backend
            // Look up remote IDs before dropping storage lock
            let remote_project_id = ProjectRepository::get_remote_id(&storage.conn, &task.project_uuid).await?;
            let remote_section_id = if let Some(section_uuid) = &task.section_uuid {
                SectionRepository::get_remote_id(&storage.conn, section_uuid).await?
            } else {
                None
            };
            let remote_parent_id = if let Some(parent_uuid) = &task.parent_uuid {
                Some(TaskRepository::get_remote_id(&storage.conn, parent_uuid).await?)
            } else {
                None
            };

            drop(storage); // Release the lock before API call

            // Create the task again via backend
            let task_args = crate::backend::CreateTaskArgs {
                content: task.content.clone(),
                description: task.description.clone().filter(|d| !d.is_empty()),
                project_remote_id: remote_project_id,
                section_remote_id: remote_section_id,
                parent_remote_id: remote_parent_id,
                priority: Some(task.priority),
                due_date: task.due_date.clone(),
                due_datetime: task.due_datetime.clone(),
                duration: task.duration.clone(),
                labels: Vec::new(), // Labels will be synced separately
            };

            let new_task = self.get_backend().await?.create_task(task_args).await?;

            // Update local storage: remove the old soft-deleted task and add the new one
            let storage = self.storage.lock().await;

            // Hard delete the old soft-deleted task
            if let Some(old_task) = TaskRepository::get_by_id(&storage.conn, task_id).await? {
                TaskRepository::delete(&storage.conn, old_task).await?;
            }

            // Store the new task (reuse the single task upsert logic)
            let txn = storage.conn.begin().await?;

            let project_uuid =
                Self::lookup_project_uuid(&txn, &self.backend_uuid, &new_task.project_remote_id, "task restore")
                    .await?;

            let section_uuid =
                Self::lookup_section_uuid(&txn, &self.backend_uuid, new_task.section_remote_id.as_ref()).await?;

            let parent_uuid = if let Some(remote_parent_id) = &new_task.parent_remote_id {
                TaskRepository::get_by_remote_id(&txn, &self.backend_uuid, remote_parent_id)
                    .await?
                    .map(|t| t.uuid)
            } else {
                None
            };

            let local_task = task::ActiveModel {
                uuid: ActiveValue::Set(Uuid::new_v4()),
                backend_uuid: ActiveValue::Set(self.backend_uuid),
                remote_id: ActiveValue::Set(new_task.remote_id),
                content: ActiveValue::Set(new_task.content),
                description: ActiveValue::Set(new_task.description),
                project_uuid: ActiveValue::Set(project_uuid),
                section_uuid: ActiveValue::Set(section_uuid),
                parent_uuid: ActiveValue::Set(parent_uuid),
                priority: ActiveValue::Set(new_task.priority),
                order_index: ActiveValue::Set(new_task.order_index),
                due_date: ActiveValue::Set(new_task.due_date),
                due_datetime: ActiveValue::Set(new_task.due_datetime),
                is_recurring: ActiveValue::Set(new_task.is_recurring),
                deadline: ActiveValue::Set(new_task.deadline),
                duration: ActiveValue::Set(new_task.duration),
                is_completed: ActiveValue::Set(new_task.is_completed),
                is_deleted: ActiveValue::Set(false),
            };

            use sea_orm::sea_query::OnConflict;
            let mut insert = task::Entity::insert(local_task);
            insert = insert.on_conflict(
                OnConflict::columns([task::Column::BackendUuid, task::Column::RemoteId])
                    .update_columns([
                        task::Column::Content,
                        task::Column::Description,
                        task::Column::ProjectUuid,
                        task::Column::SectionUuid,
                        task::Column::ParentUuid,
                        task::Column::Priority,
                        task::Column::OrderIndex,
                        task::Column::DueDate,
                        task::Column::DueDatetime,
                        task::Column::IsRecurring,
                        task::Column::Deadline,
                        task::Column::Duration,
                        task::Column::IsCompleted,
                        task::Column::IsDeleted,
                    ])
                    .to_owned(),
            );
            insert.exec(&txn).await?;

            txn.commit().await?;
        } else {
            // For completed tasks, just reopen them
            let remote_id = task.remote_id.clone();
            drop(storage); // Release the lock before API call
            self.get_backend().await?.reopen_task(&remote_id).await?;

            // Clear local completion flag
            let storage = self.storage.lock().await;

            if let Some(task) = TaskRepository::get_by_id(&storage.conn, task_id).await? {
                let mut active_model: task::ActiveModel = task.into_active_model();
                active_model.is_completed = ActiveValue::Set(false);
                TaskRepository::update(&storage.conn, active_model).await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::quick_add_move_target;

    #[test]
    fn inbox_result_moves_to_the_dialog_project() {
        assert_eq!(
            quick_add_move_target("inbox", Some("inbox"), Some("work")),
            Some("work")
        );
    }

    #[test]
    fn project_named_in_the_text_wins() {
        assert_eq!(quick_add_move_target("home", Some("inbox"), Some("work")), None);
    }

    #[test]
    fn no_dialog_project_leaves_the_task_alone() {
        assert_eq!(quick_add_move_target("inbox", Some("inbox"), None), None);
    }

    #[test]
    fn dialog_project_already_reached_needs_no_move() {
        assert_eq!(quick_add_move_target("inbox", Some("inbox"), Some("inbox")), None);
    }

    #[test]
    fn unknown_inbox_never_moves() {
        assert_eq!(quick_add_move_target("p1", None, Some("work")), None);
    }
}
