use chrono::Utc;
use cucumber::{given, then};
use graphql_client::{GraphQLQuery, Response};
use pretty_assertions::assert_eq;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{any, body_json, header, method, path},
};

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionId,
        config::IntegrationConnectionConfig,
        integrations::{
            linear::{LinearConfig, LinearSyncTaskConfig},
            todoist::TodoistConfig,
        },
    },
    notification::{NotificationSourceKind, NotificationStatus},
    task::{PresetDueDate, ProjectSummary, Task, TaskId, TaskStatus},
    third_party::{
        integrations::{
            linear::{LinearIssue, LinearWorkflowState, LinearWorkflowStateType},
            todoist::TodoistItem,
        },
        item::ThirdPartyItemData,
    },
    user::UserId,
};
use universal_inbox_api::integrations::{
    linear::graphql::{
        IssueUpdateState,
        issue_update_state::{self, IssueUpdateStateIssueUpdate},
    },
    oauth2::AccessToken,
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection,
            linear_oauth_credential, todoist_oauth_credential,
        },
        notification::{linear::sync_linear_tasks_response, list_notifications_with_tasks},
        rest::get_resource,
        task::{linear::create_linear_task, todoist::sync_todoist_items_response},
    },
    steps::{status::UserStatus, todoist_task_changes::TaskState},
    world::ApiWorld,
};

const MY_LINEAR_TOKEN: &str = "linear_test_access_token";

fn inbox() -> ProjectSummary {
    ProjectSummary {
        name: "Inbox".to_string(),
        source_id: "1111".into(),
    }
}

fn linear_config() -> IntegrationConnectionConfig {
    IntegrationConnectionConfig::Linear(LinearConfig {
        sync_notifications_enabled: true,
        sync_task_config: LinearSyncTaskConfig {
            enabled: true,
            target_project: Some(inbox()),
            default_due_at: Some(PresetDueDate::Today),
            task_manager_provider_kind: None,
            ..Default::default()
        },
    })
}

fn the_reported_item_id() -> String {
    sync_todoist_items_response().items.unwrap()[1].id.clone()
}

fn the_issue(state: TaskStatus) -> LinearIssue {
    let issues: Vec<LinearIssue> = sync_linear_tasks_response()
        .data
        .unwrap()
        .try_into()
        .unwrap();
    let issue = issues.into_iter().next().unwrap();
    let (state_type, completed_at) = match state {
        TaskStatus::Done => (LinearWorkflowStateType::Completed, Some(Utc::now())),
        TaskStatus::Active => (LinearWorkflowStateType::Unstarted, None),
        TaskStatus::Deleted => panic!("A deleted Linear issue is not part of these scenarios"),
    };
    LinearIssue {
        state: LinearWorkflowState {
            r#type: state_type,
            ..issue.state.clone()
        },
        completed_at,
        ..issue
    }
}

async fn mirror(
    world: &mut ApiWorld,
    user_id: UserId,
    linear_connection_id: IntegrationConnectionId,
    todoist_connection_id: IntegrationConnectionId,
    state: TaskStatus,
) -> Task {
    let app = world.app.get().await;
    create_linear_task(
        app,
        &the_issue(state),
        inbox(),
        user_id,
        linear_connection_id,
        todoist_connection_id,
        the_reported_item_id(),
    )
    .await
}

#[given("I am connected to Linear, syncing its tasks into the Inbox of Todoist, due today")]
async fn i_am_connected_to_linear(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        linear_config(),
        &settings(),
        linear_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.linear_connection = Some(connection);
}

#[given(
    expr = "I have the task of a Linear issue that is {task_state} in Linear, mirrored by the item that Todoist reports"
)]
async fn i_have_the_mirrored_task(world: &mut ApiWorld, state: TaskState) {
    let user_id = world.user().id;
    let linear_id = world
        .integrations
        .linear_connection
        .as_ref()
        .expect("The account is not connected to Linear")
        .id;
    let todoist_id = world
        .integrations
        .todoist_connection
        .as_ref()
        .expect("The account is not connected to Todoist")
        .id;
    let task = mirror(world, user_id, linear_id, todoist_id, state.0).await;
    assert_eq!(task.status, state.0);
    world.linear_tasks.task = Some(task);
}

#[given(
    "that other user has connections to Todoist and Linear of their own, and their own active task of the same Linear issue, mirrored by a Todoist item with the same id"
)]
async fn the_other_user_has_the_same_mirrored_task(world: &mut ApiWorld) {
    let user_id = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id;
    let app = world.app.get().await;
    let todoist = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings(),
        OAuthCredentialFixture {
            access_token: AccessToken("other_user_todoist_access_token".to_string()),
            ..todoist_oauth_credential()
        },
        None,
        None,
    )
    .await;
    let linear = create_and_mock_integration_connection(
        app,
        user_id,
        linear_config(),
        &settings(),
        OAuthCredentialFixture {
            access_token: AccessToken("other_user_linear_access_token".to_string()),
            ..linear_oauth_credential()
        },
        None,
        None,
    )
    .await;
    let task = mirror(world, user_id, linear.id, todoist.id, TaskStatus::Active).await;
    world.linear_tasks_from_todoist.others_task = Some(task);
}

#[given(expr = "Linear moves that issue to its {task_state} state, once and with my access token")]
async fn linear_moves_the_issue(world: &mut ApiWorld, state: TaskState) {
    let issue = the_issue(state.0);
    let state_id = issue
        .get_state_id_for_task_status(state.0)
        .expect("The issue has no state for that status");
    let body = IssueUpdateState::build_query(issue_update_state::Variables {
        id: issue.id.to_string(),
        state_id: state_id.to_string(),
    });
    let app = world.app.get().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(&body))
        .and(header(
            "authorization",
            format!("Bearer {MY_LINEAR_TOKEN}").as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(Response {
                    data: Some(issue_update_state::ResponseData {
                        issue_update: IssueUpdateStateIssueUpdate { success: true },
                    }),
                    errors: None,
                    extensions: None,
                }),
        )
        .expect(1)
        .mount(&app.linear_mock_server)
        .await;
}

#[given(expr = "Todoist reports only the item that mirrors that task, renamed {string}")]
fn todoist_reports_the_renamed_item(world: &mut ApiWorld, title: String) {
    let task = world.linear_tasks.task.as_ref().expect("I have no task");
    let ThirdPartyItemData::TodoistItem(item) = task
        .sink_item
        .as_ref()
        .expect("That task is not mirrored")
        .data
        .clone()
    else {
        panic!("That task is not mirrored by a Todoist item");
    };
    let mut response = sync_todoist_items_response();
    response.items = Some(vec![TodoistItem {
        content: title,
        ..*item
    }]);
    world.todoist_tasks.reported = Some(response);
}

fn my_task_id(world: &ApiWorld) -> TaskId {
    world.linear_tasks.task.as_ref().expect("I have no task").id
}

async fn my_task(world: &mut ApiWorld) -> Box<Task> {
    let id = my_task_id(world);
    let client = world.client();
    let app = world.app.get().await;
    get_resource(&client, &app.api_address, "tasks", id.into()).await
}

#[then(
    expr = "the task of that Linear issue, read again, is {task_state}, and still the same task"
)]
async fn my_task_is(world: &mut ApiWorld, state: TaskState) {
    let before = world.linear_tasks.task.clone().unwrap();
    let task = my_task(world).await;
    assert_eq!(
        (task.id, task.source_item.source_id.clone(), task.status),
        (before.id, before.source_item.source_id, state.0)
    );
}

#[then(expr = "the task of that Linear issue, read again, is titled {string}")]
async fn my_task_is_titled(world: &mut ApiWorld, title: String) {
    assert_eq!(my_task(world).await.title, title);
}

#[then("the sync brought back the task of that Linear issue, alone")]
fn the_sync_brought_back_my_task(world: &mut ApiWorld) {
    let ids: Vec<TaskId> = world
        .todoist_tasks
        .results
        .iter()
        .map(|creation| creation.task.id)
        .collect();
    assert_eq!(ids, vec![my_task_id(world)]);
}

async fn the_notifications_of_my_task(
    world: &mut ApiWorld,
    kind: Option<NotificationSourceKind>,
) -> Vec<NotificationStatus> {
    let id = my_task_id(world);
    let client = world.client();
    let app = world.app.get().await;
    list_notifications_with_tasks(
        &client,
        &app.api_address,
        vec![],
        false,
        Some(id),
        kind,
        false,
    )
    .await
    .into_iter()
    .map(|notification| notification.status)
    .collect()
}

#[then(expr = "the task of that Linear issue has one Todoist notification, which is {status}")]
async fn my_task_has_one_notification(world: &mut ApiWorld, status: UserStatus) {
    assert_eq!(
        the_notifications_of_my_task(world, Some(NotificationSourceKind::Todoist)).await,
        vec![status.0]
    );
}

#[then("the task of that Linear issue has no notification")]
async fn my_task_has_no_notification(world: &mut ApiWorld) {
    assert_eq!(the_notifications_of_my_task(world, None).await, vec![]);
}

#[then("the task of that other user, read by that other user, is exactly as it was")]
async fn the_other_task_is_unchanged(world: &mut ApiWorld) {
    let before = world
        .linear_tasks_from_todoist
        .others_task
        .clone()
        .expect("That other user has no task");
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    let task: Box<Task> = get_resource(&client, &app.api_address, "tasks", before.id.into()).await;
    assert_eq!(*task, before);
}

#[given("Linear is not asked anything more")]
async fn linear_is_not_asked_anything_more(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&app.linear_mock_server)
        .await;
}

#[then("the sync brought back only tasks of mine")]
async fn the_sync_brought_back_only_my_tasks(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let others = world
        .linear_tasks_from_todoist
        .others_task
        .as_ref()
        .expect("That other user has no task")
        .id;
    assert!(!world.todoist_tasks.results.is_empty());
    for creation in &world.todoist_tasks.results {
        assert_eq!(creation.task.user_id, user_id);
        assert_ne!(creation.task.id, others);
    }
}
