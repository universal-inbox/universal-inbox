use chrono::{TimeDelta, Utc};
use cucumber::{gherkin::Step, given, then, when};
use pretty_assertions::assert_eq;
use reqwest::Client;
use secrecy::ExposeSecret;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::{
    HasHtmlUrl, Page, PageToken,
    audit::{
        AuditAction, AuditActor, AuditEntityKind, AuditEvent, AuditEventGroup, AuditUserChannel,
    },
    integration_connection::provider::IntegrationProviderKind,
    notification::{NotificationSourceKind, NotificationStatus, service::NotificationPatch},
    task::{DueDate, TaskCreation, TaskSourceKind},
    third_party::integrations::todoist::TodoistItem,
};

use crate::{
    helpers::{
        TestedApp,
        notification::create_task_from_notification,
        task::{
            sync_tasks,
            todoist::{
                mock_todoist_get_item_service, mock_todoist_item_add_service,
                mock_todoist_sync_resources_service, sync_todoist_items_response, todoist_item,
            },
        },
    },
    world::ApiWorld,
};

pub async fn list_events(client: &Client, app: &TestedApp, query: &str) -> Page<AuditEvent> {
    client
        .get(format!("{}audit-events?{query}", app.api_address))
        .send()
        .await
        .expect("Failed to list audit events")
        .json()
        .await
        .expect("Failed to parse audit events")
}

async fn list_groups(client: &Client, app: &TestedApp, query: &str) -> Page<AuditEventGroup> {
    client
        .get(format!("{}audit-events/groups?{query}", app.api_address))
        .send()
        .await
        .expect("Failed to list audit event groups")
        .json()
        .await
        .expect("Failed to parse audit event groups")
}

fn next_page_query(query: &str, token: &Option<PageToken>) -> String {
    format!(
        "{query}&page_token={}",
        token
            .as_ref()
            .expect("There is no next page")
            .to_url_parameter()
            .unwrap()
    )
}

fn parse_actor(description: &str) -> AuditActor {
    match description {
        "API key" => AuditActor::ApiKey,
        other => match other.strip_prefix("extension ") {
            Some(version) => AuditActor::Extension {
                version: version.to_string(),
            },
            None => panic!("Unknown actor {other:?}"),
        },
    }
}

fn web_user() -> AuditActor {
    AuditActor::User {
        via: AuditUserChannel::Web,
    }
}

fn expected_status_change() -> serde_json::Value {
    json!({ "status": { "from": "Unread", "to": "Deleted" } })
}

async fn list(world: &mut ApiWorld, query: &str) {
    let client = world.client();
    let app = world.app.get().await;
    world.audit.page = Some(list_events(&client, app, query).await);
}

#[when(expr = "I list my audit events with the query {string}")]
async fn i_list_my_audit_events(world: &mut ApiWorld, query: String) {
    list(world, &query).await;
}

#[when(expr = "I list the audit events of the notification {string}")]
async fn i_list_the_audit_events_of_a_notification(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    list(world, &format!("entity_id={id}")).await;
}

#[when(expr = "I list the audit events of the notification {string} that snoozed it")]
async fn i_list_the_snooze_events(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    list(world, &format!("entity_id={id}&change=Snoozed")).await;
}

#[when(expr = "I list the audit events of an id nobody uses")]
async fn i_list_the_audit_events_of_an_unused_id(world: &mut ApiWorld) {
    list(world, &format!("entity_id={}", Uuid::new_v4())).await;
}

#[when(expr = "that other user lists their audit events with the query {string}")]
async fn that_other_user_lists_their_audit_events(world: &mut ApiWorld, query: String) {
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    world.audit.page = Some(list_events(&client, app, &query).await);
}

#[when(expr = "that other user lists the audit events of the notification {string}")]
async fn that_other_user_lists_the_audit_events_of(world: &mut ApiWorld, name: String) {
    let id = world.notifications.named(&name).id;
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    world.audit.page = Some(list_events(&client, app, &format!("entity_id={id}")).await);
}

#[when(expr = "I list the next page of my audit events with the query {string}")]
async fn i_list_the_next_page_of_my_audit_events(world: &mut ApiWorld, query: String) {
    let client = world.client();
    let token = world.audit.page.as_ref().unwrap().next_page_token.clone();
    let app = world.app.get().await;
    world.audit.second_page =
        Some(list_events(&client, app, &next_page_query(&query, &token)).await);
}

#[when(
    expr = "I search my audit events for the first word of the notification {string}, in lowercase"
)]
async fn i_search_for_the_first_word_of_a_title(world: &mut ApiWorld, name: String) {
    let title = world.notifications.named(&name).title.clone();
    let word = title.split_whitespace().next().unwrap().to_lowercase();
    list(world, &format!("query={word}")).await;
}

#[when("I search my audit events for a title that does not exist")]
async fn i_search_for_a_title_that_does_not_exist(world: &mut ApiWorld) {
    list(world, "query=no%25such_title").await;
}

#[when(expr = "I list my audit event groups with the query {string}")]
async fn i_list_my_audit_event_groups(world: &mut ApiWorld, query: String) {
    let client = world.client();
    let app = world.app.get().await;
    world.audit.groups = Some(list_groups(&client, app, &query).await);
}

#[when(expr = "I list the next page of my audit event groups with the query {string}")]
async fn i_list_the_next_page_of_my_audit_event_groups(world: &mut ApiWorld, query: String) {
    let client = world.client();
    let token = world.audit.groups.as_ref().unwrap().next_page_token.clone();
    let app = world.app.get().await;
    world.audit.second_groups =
        Some(list_groups(&client, app, &next_page_query(&query, &token)).await);
}

#[when(expr = "I snooze {string} until {string} with my API token")]
async fn i_snooze_with_my_api_token(world: &mut ApiWorld, name: String, time: String) {
    snooze_with_token(world, name, time, None).await;
}

#[when(
    expr = "I snooze {string} until {string} with my API token, as the extension in version {string}"
)]
async fn i_snooze_as_the_extension(
    world: &mut ApiWorld,
    name: String,
    time: String,
    version: String,
) {
    snooze_with_token(world, name, time, Some(version)).await;
}

async fn snooze_with_token(
    world: &mut ApiWorld,
    name: String,
    time: String,
    extension_version: Option<String>,
) {
    let id = world.notifications.named(&name).id;
    let jwt = world
        .sessions
        .api_token
        .as_ref()
        .expect("The scenario has no API token")
        .jwt_token
        .expose_secret()
        .0
        .clone();
    let snoozed_until = chrono::DateTime::parse_from_rfc3339(&time)
        .unwrap()
        .with_timezone(&Utc);
    let app = world.app.get().await;
    let mut request = Client::new()
        .patch(format!("{}notifications/{id}", app.api_address))
        .bearer_auth(jwt)
        .json(&NotificationPatch {
            snoozed_until: Some(snoozed_until),
            ..Default::default()
        });
    if let Some(version) = extension_version {
        request = request.header("X-Universal-Inbox-Client", format!("extension/{version}"));
    }
    assert_eq!(request.send().await.unwrap().status(), 200);
}

#[given("Todoist has its items to sync")]
async fn todoist_has_its_items_to_sync(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "items",
        &sync_todoist_items_response(),
        None,
    )
    .await;
}

#[when("I sync my Todoist tasks")]
async fn i_sync_my_todoist_tasks(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let _: Vec<universal_inbox::task::TaskCreationResult> = sync_tasks(
        &client,
        &app.api_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;
}

#[given(expr = "GitHub accepts to delete the notification threads {int}, {int} and {int}")]
async fn github_accepts_to_delete_three_threads(
    world: &mut ApiWorld,
    first: u32,
    second: u32,
    third: u32,
) {
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    let app = world.app.get().await;
    for thread in [first, second, third] {
        Mock::given(method("DELETE"))
            .and(path(format!("/notifications/threads/{thread}")))
            .respond_with(ResponseTemplate::new(205))
            .mount(&app.github_mock_server)
            .await;
    }
}

#[given(expr = "GitHub accepts to delete the notification thread {int}")]
async fn github_accepts_to_delete_a_thread(world: &mut ApiWorld, thread: u32) {
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    let app = world.app.get().await;
    Mock::given(method("DELETE"))
        .and(path(format!("/notifications/threads/{thread}")))
        .respond_with(ResponseTemplate::new(205))
        .mount(&app.github_mock_server)
        .await;
}

#[when("I delete all my unread GitHub notifications")]
async fn i_delete_all_my_unread_github_notifications(world: &mut ApiWorld) {
    use universal_inbox::notification::service::PatchNotificationsRequest;
    let client = world.client();
    let app = world.app.get().await;
    let _: Vec<universal_inbox::notification::Notification> =
        crate::helpers::rest::patch_resource_collection(
            &client,
            &app.api_address,
            "notifications",
            &PatchNotificationsRequest {
                status: vec![NotificationStatus::Unread],
                sources: vec![NotificationSourceKind::Github],
                patch: NotificationPatch {
                    status: Some(NotificationStatus::Deleted),
                    ..Default::default()
                },
            },
        )
        .await;
}

#[given(
    expr = "Todoist accepts a task for the notification {string} in the project {string}, whose description must stay secret"
)]
async fn todoist_accepts_a_task(world: &mut ApiWorld, name: String, project_name: String) {
    let notification = world.notifications.named(&name).clone();
    let secret = "Secret description of the task".to_string();
    let item = TodoistItem {
        project_id: "2222".to_string(),
        description: secret.clone(),
        ..*todoist_item()
    };
    let due_at: Option<DueDate> = item.due.as_ref().map(|due| due.into());
    let body = format!(
        "- [{}]({})",
        notification.title,
        notification.get_html_url().as_ref()
    );
    assert_eq!(project_name, "Project2");
    let app = world.app.get().await;
    mock_todoist_item_add_service(
        &app.todoist_mock_server,
        &item.id,
        item.content.clone(),
        Some(body),
        Some(item.project_id.clone()),
        due_at.as_ref().map(|due_at| due_at.into()),
        item.priority,
    )
    .await;
    mock_todoist_get_item_service(&app.todoist_mock_server, Box::new(item)).await;
    world.audit.secret_description = Some(secret);
}

#[when(expr = "I create the task from the notification {string} in the project {string}")]
async fn i_create_the_task(world: &mut ApiWorld, name: String, project_name: String) {
    let notification = world.notifications.named(&name).clone();
    let item = TodoistItem {
        project_id: "2222".to_string(),
        description: world.audit.secret_description.clone().unwrap(),
        ..*todoist_item()
    };
    let due_at: Option<DueDate> = item.due.as_ref().map(|due| due.into());
    let body = format!(
        "- [{}]({})",
        notification.title,
        notification.get_html_url().as_ref()
    );
    let client = world.client();
    let app = world.app.get().await;
    let notification_with_task = create_task_from_notification(
        &client,
        &app.api_address,
        notification.id,
        Some(TaskCreation {
            title: item.content.clone(),
            body: Some(body),
            project_name: Some(project_name),
            due_at,
            priority: item.priority.into(),
            task_provider_kind: None,
            time_config: None,
        }),
    )
    .await
    .unwrap();
    let task = notification_with_task.task.unwrap();
    let response = client
        .get(format!(
            "{}audit-events?entity_id={}",
            app.api_address, task.id
        ))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    world.audit.task = Some(task);
    world.audit.task_audit_body = Some(response);
}

#[then(
    "the audit events of the task do not contain the secret description, which is the task body"
)]
fn the_audit_events_do_not_leak_the_description(world: &mut ApiWorld) {
    let secret = world.audit.secret_description.as_ref().unwrap();
    let response = world.audit.task_audit_body.as_ref().unwrap();
    assert_eq!(&world.audit.task.as_ref().unwrap().body, secret);
    assert!(
        !response.contains(secret),
        "the task body leaked: {response}"
    );
}

fn task_events(world: &ApiWorld) -> Page<AuditEvent> {
    serde_json::from_str(world.audit.task_audit_body.as_ref().unwrap()).unwrap()
}

#[then(
    expr = "the task creation event is by me through the web and records the title, the project {string}, the kind {string} and that a body is present"
)]
fn the_task_creation_event_records_its_parameters(
    world: &mut ApiWorld,
    project: String,
    kind: String,
) {
    let events = task_events(world);
    let creation = events
        .content
        .iter()
        .find(|event| event.entity_kind == AuditEntityKind::Task)
        .expect("Expected the task creation event");
    assert_eq!(creation.action, AuditAction::Created);
    assert_eq!(creation.actor, web_user());
    let params = creation.params.as_ref().unwrap();
    assert_eq!(params["title"], json!(todoist_item().content));
    assert_eq!(params["project"], json!(project));
    assert_eq!(params["kind"], json!(kind));
    assert_eq!(params["body_present"], json!(true));
}

#[then(expr = "the history of the task includes the notification {string}, linked to the task")]
fn the_task_history_includes_the_notification(world: &mut ApiWorld, name: String) {
    let notification_id = world.notifications.named(&name).id;
    let task = world.audit.task.as_ref().unwrap();
    let events = task_events(world);
    let link = events
        .content
        .iter()
        .find(|event| event.entity_kind == AuditEntityKind::Notification)
        .expect("Expected the notification linked to the task");
    assert_eq!(link.entity_id, notification_id.0);
    assert_eq!(link.related_entity_id, Some(task.id.0));
    assert_eq!(
        link.changes["task_id"].to,
        Some(json!(task.id.0.to_string()))
    );
}

#[then(
    expr = "the newest event is an update of the notification {string} on GitHub by me through the web, snoozing it until {string} from no snooze"
)]
fn the_newest_event_is_the_snooze(world: &mut ApiWorld, name: String, time: String) {
    let notification = world.notifications.named(&name).clone();
    let snooze = &world.audit.events()[0];
    assert_eq!(snooze.entity_kind, AuditEntityKind::Notification);
    assert_eq!(snooze.entity_id, notification.id.0);
    assert_eq!(snooze.action, AuditAction::Updated);
    assert_eq!(snooze.title, notification.title);
    assert_eq!(snooze.source_kind.as_deref(), Some("Github"));
    assert_eq!(snooze.actor, web_user());
    let expected_time = time.trim_end_matches('Z');
    assert_eq!(
        serde_json::to_value(&snooze.changes).unwrap(),
        json!({ "snoozed_until": { "from": null, "to": expected_time } })
    );
}

#[then(
    "the oldest event is the creation of the notification, with the status going from nothing to \"Unread\""
)]
fn the_oldest_event_is_the_creation(world: &mut ApiWorld) {
    let creation = world.audit.events().last().unwrap();
    assert_eq!(creation.action, AuditAction::Created);
    assert_eq!(
        serde_json::to_value(&creation.changes).unwrap(),
        json!({ "status": { "from": null, "to": "Unread" } })
    );
}

#[then("the actors of the events are, newest first:")]
fn the_actors_of_the_events_are(world: &mut ApiWorld, step: &Step) {
    let expected: Vec<AuditActor> = step
        .table
        .as_ref()
        .expect("The actors must be given as a table")
        .rows
        .iter()
        .map(|row| parse_actor(&row[0]))
        .collect();
    let actors: Vec<AuditActor> = world
        .audit
        .events()
        .iter()
        .map(|e| e.actor.clone())
        .collect();
    assert_eq!(actors, expected);
}

#[then(
    "every listed event is a creation by the SyncTasks job for Todoist, triggered by me through the web"
)]
fn every_event_is_a_system_creation(world: &mut ApiWorld) {
    assert!(!world.audit.events().is_empty());
    for event in world.audit.events() {
        assert_eq!(
            event.actor,
            AuditActor::System {
                job: "SyncTasks".to_string(),
                source: Some(IntegrationProviderKind::Todoist),
                triggered_by: Some(Box::new(web_user())),
            }
        );
    }
}

#[then(expr = "{int} audit events are listed in total")]
fn events_are_listed_in_total(world: &mut ApiWorld, total: usize) {
    assert_eq!(world.audit.page.as_ref().unwrap().total, total);
}

#[then(
    "the first two events share their transaction, and each changes the status from \"Unread\" to \"Deleted\""
)]
fn the_events_share_their_transaction(world: &mut ApiWorld) {
    let events = world.audit.events();
    assert_eq!(events[0].transaction_id, events[1].transaction_id);
    for event in events {
        assert_eq!(
            serde_json::to_value(&event.changes).unwrap(),
            expected_status_change()
        );
    }
}

#[then(expr = "the page holds {int} events and links to the next page at the offset {int}")]
fn the_page_links_to_the_next_page(world: &mut ApiWorld, count: usize, offset: usize) {
    let page = world.audit.page.as_ref().unwrap();
    assert_eq!(page.content.len(), count);
    assert_eq!(page.next_page_token, Some(PageToken::Offset(offset)));
}

#[then(
    expr = "the second page holds {int} events, has no next page and ends with the creation of the notification"
)]
fn the_second_page_is_the_last(world: &mut ApiWorld, count: usize) {
    let page = world.audit.second_page.as_ref().unwrap();
    assert_eq!(page.content.len(), count);
    assert_eq!(page.next_page_token, None);
    assert_eq!(page.content[1].action, AuditAction::Created);
}

#[then(
    expr = "the groups page holds {int} of {int} groups over {int} pages and links to the next page at the offset {int}"
)]
fn the_groups_page_is_described(
    world: &mut ApiWorld,
    count: usize,
    total: usize,
    pages: usize,
    offset: usize,
) {
    let page = world.audit.groups.as_ref().unwrap();
    assert_eq!(page.total, total);
    assert_eq!(page.pages_count, pages);
    assert_eq!(page.content.len(), count);
    assert_eq!(page.next_page_token, Some(PageToken::Offset(offset)));
}

#[then(
    expr = "the first group is the bulk deletion of {int} events sharing their transaction, each changing the status from \"Unread\" to \"Deleted\""
)]
fn the_first_group_is_the_bulk_deletion(world: &mut ApiWorld, count: usize) {
    let first = &world.audit.groups.as_ref().unwrap().content[0].events;
    assert_eq!(first.len(), count);
    for event in first {
        assert_eq!(event.transaction_id, first[0].transaction_id);
        assert_eq!(
            serde_json::to_value(&event.changes).unwrap(),
            expected_status_change()
        );
    }
    assert_eq!(
        world.audit.groups.as_ref().unwrap().content[1].events.len(),
        1
    );
}

#[then(
    expr = "the second groups page holds {int} groups and no next page, and the three snoozes follow, newest first"
)]
fn the_second_groups_page_holds_the_snoozes(world: &mut ApiWorld, count: usize) {
    let first = world.audit.groups.as_ref().unwrap();
    let second = world.audit.second_groups.as_ref().unwrap();
    assert_eq!(second.content.len(), count);
    assert_eq!(second.next_page_token, None);
    let snoozes: Vec<AuditEvent> = first.content[1..]
        .iter()
        .chain(&second.content)
        .map(|group| {
            assert_eq!(group.events.len(), 1);
            group.events[0].clone()
        })
        .collect();
    assert_eq!(
        snoozes
            .iter()
            .map(|event| event.changes["snoozed_until"].to.clone())
            .collect::<Vec<_>>(),
        [3, 2, 1]
            .map(|day| Some(json!(format!("2064-01-0{day}T09:00:00"))))
            .to_vec()
    );
}

#[when("the creation events grow 8 days old, which is past the 7 days of retention")]
async fn the_creation_events_are_expired(world: &mut ApiWorld) {
    let expired_at = Utc::now() - TimeDelta::days(8);
    let app = world.app.get().await;
    sqlx::query("UPDATE audit_event SET created_at = $1 WHERE action = 'Created'")
        .bind(expired_at)
        .execute(&*app.repository.pool)
        .await
        .unwrap();
}

#[when("the expired audit events are purged")]
async fn the_expired_audit_events_are_purged(world: &mut ApiWorld) {
    let app = world.app.get().await;
    world.audit.purged = Some(
        app.audit_service
            .purge_expired_audit_events()
            .await
            .unwrap(),
    );
}

#[then(expr = "{int} audit event was deleted")]
fn audit_events_were_deleted(world: &mut ApiWorld, count: u64) {
    assert_eq!(world.audit.purged, Some(count));
}

#[then("the only audit event left is an update")]
fn the_only_event_left_is_an_update(world: &mut ApiWorld) {
    let page = world.audit.page.as_ref().unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.content[0].action, AuditAction::Updated);
}
