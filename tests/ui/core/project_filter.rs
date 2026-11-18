use terminalist::entities::{project, task};
use terminalist::ui::app_component::AppState;
use terminalist::ui::core::project_filter::{filter_tasks, picker_entries, subtree, tree_order};
use terminalist::ui::core::SidebarSelection;
use uuid::Uuid;

fn project_model(name: &str, parent: Option<Uuid>) -> project::Model {
    project::Model {
        uuid: Uuid::new_v4(),
        backend_uuid: Uuid::nil(),
        remote_id: name.to_string(),
        name: name.to_string(),
        is_favorite: false,
        is_inbox_project: false,
        order_index: 0,
        parent_uuid: parent,
    }
}

fn task_model(content: &str, project: Uuid) -> task::Model {
    task::Model {
        uuid: Uuid::new_v4(),
        backend_uuid: Uuid::nil(),
        remote_id: content.to_string(),
        content: content.to_string(),
        description: None,
        project_uuid: project,
        section_uuid: None,
        parent_uuid: None,
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

/// Work > Clients > Acme, plus an unrelated Home project.
fn fixture() -> (Vec<project::Model>, [Uuid; 4]) {
    let work = project_model("Work", None);
    let clients = project_model("Clients", Some(work.uuid));
    let acme = project_model("Acme", Some(clients.uuid));
    let home = project_model("Home", None);
    let ids = [work.uuid, clients.uuid, acme.uuid, home.uuid];
    // Children listed before their parents, so the subtree walk can't rely on order.
    (vec![acme, home, clients, work], ids)
}

#[test]
fn subtree_reaches_every_nesting_level() {
    let (projects, [work, clients, acme, home]) = fixture();
    let members = subtree(&projects, work);
    assert!(members.contains(&work) && members.contains(&clients) && members.contains(&acme));
    assert!(!members.contains(&home));
}

#[test]
fn filter_keeps_tasks_from_the_project_and_its_subprojects() {
    let (projects, [work, _, acme, home]) = fixture();
    let tasks = vec![
        task_model("standup", work),
        task_model("invoice", acme),
        task_model("laundry", home),
    ];
    let kept: Vec<String> = filter_tasks(&tasks, &projects, work).into_iter().map(|t| t.content).collect();
    assert_eq!(kept, vec!["standup", "invoice"]);
}

#[test]
fn tree_order_puts_inbox_first_and_children_under_their_parent() {
    let (mut projects, [work, clients, acme, home]) = fixture();
    let mut inbox = project_model("Inbox", None);
    inbox.is_inbox_project = true;
    let inbox_uuid = inbox.uuid;
    projects.push(inbox);

    let order: Vec<(Uuid, usize)> = tree_order(&projects).into_iter().map(|(p, d)| (p.uuid, d)).collect();
    assert_eq!(
        order,
        vec![(inbox_uuid, 0), (home, 0), (work, 0), (clients, 1), (acme, 2)]
    );
}

#[test]
fn picker_narrows_by_name_and_always_offers_all_projects() {
    let (projects, [_, _, acme, _]) = fixture();
    let entries = picker_entries(&projects, "ACM");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].project, None);
    assert_eq!(entries[1].project, Some(acme));
}

#[test]
fn app_state_filters_date_views_but_not_project_views() {
    let (projects, [work, _, _, home]) = fixture();
    let mut state = AppState {
        projects,
        tasks: vec![task_model("standup", work), task_model("laundry", home)],
        project_filter: Some(work),
        sidebar_selection: SidebarSelection::Upcoming,
        ..Default::default()
    };
    assert_eq!(state.visible_tasks().len(), 1);
    assert_eq!(state.project_filter_name().as_deref(), Some("Work"));

    state.sidebar_selection = SidebarSelection::Project(home);
    assert_eq!(state.visible_tasks().len(), 2);
    assert_eq!(state.project_filter_name(), None);
}

#[test]
fn filter_on_a_deleted_project_is_dropped() {
    let (projects, _) = fixture();
    let mut state = AppState {
        projects,
        project_filter: Some(Uuid::new_v4()),
        ..Default::default()
    };
    assert!(state.drop_stale_project_filter());
    assert_eq!(state.project_filter, None);
}
