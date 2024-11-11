use chrono::{TimeDelta, Timelike, Utc};

use universal_inbox::{
    integration_connection::IntegrationConnectionId,
    task::{service::TaskPatch, PresetDueDate, ProjectSummary, Task, TaskCreation, TaskPriority},
    third_party::{
        integrations::{linear::LinearIssue, todoist::TodoistItem},
        item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    },
    user::UserId,
};

use universal_inbox_api::{
    integrations::task::ThirdPartyTaskService,
    repository::{task::TaskRepository, third_party::ThirdPartyItemRepository},
};

use crate::helpers::TestedApp;

pub async fn create_linear_task(
    app: &TestedApp,
    linear_issue: &LinearIssue,
    project: ProjectSummary,
    user_id: UserId,
    source_integration_connection_id: IntegrationConnectionId,
    sink_integration_connection_id: IntegrationConnectionId,
    todoist_source_id: String,
) -> Task {
    let mut transaction = app.repository.begin().await.unwrap();
    let source_third_party_item = ThirdPartyItem::new(
        linear_issue.id.to_string(),
        ThirdPartyItemData::LinearIssue(Box::new(linear_issue.clone())),
        user_id,
        source_integration_connection_id,
    );
    let source_third_party_item = app
        .repository
        .create_or_update_third_party_item(&mut transaction, Box::new(source_third_party_item))
        .await
        .unwrap()
        .value();

    let task_request = app
        .task_service
        .read()
        .await
        .linear_service
        .third_party_item_into_task(
            &mut transaction,
            linear_issue,
            &source_third_party_item,
            Some(TaskCreation {
                title: "".to_string(),
                body: None,
                project: project.clone(),
                due_at: Some(PresetDueDate::Today.into()),
                priority: TaskPriority::P1,
            }),
            user_id,
        )
        .await
        .unwrap();
    let todoist_item = TodoistItem {
        id: todoist_source_id,
        parent_id: None,
        project_id: project.source_id.clone(),
        sync_id: None,
        section_id: None,
        content: task_request.title.clone(),
        description: task_request.body.clone(),
        labels: vec![],
        child_order: 1,
        day_order: None,
        priority: task_request.priority.into(),
        checked: false,
        is_deleted: false,
        collapsed: false,
        completed_at: None,
        added_at: Utc::now().with_nanosecond(0).unwrap(),
        due: task_request
            .due_at
            .clone()
            .into_value()
            .map(|due_at| (&due_at).into()),
        user_id: "user_id".to_string(),
        added_by_uid: None,
        assigned_by_uid: None,
        responsible_uid: None,
    };

    let upsert_task = app
        .repository
        .create_or_update_task(&mut transaction, task_request)
        .await
        .unwrap();

    let mut sink_third_party_item =
        todoist_item.into_third_party_item(user_id, sink_integration_connection_id);
    // Make sure it will be updated
    sink_third_party_item.updated_at = Utc::now() - TimeDelta::seconds(1);
    let sink_third_party_item = app
        .repository
        .create_or_update_third_party_item(&mut transaction, Box::new(sink_third_party_item))
        .await
        .unwrap();

    let mut task = upsert_task.value();
    task.sink_item = Some(*sink_third_party_item.value());
    app.repository
        .update_task(
            &mut transaction,
            task.id,
            &TaskPatch {
                sink_item_id: Some(task.sink_item.as_ref().unwrap().id),
                ..Default::default()
            },
            user_id,
        )
        .await
        .unwrap();

    transaction.commit().await.unwrap();

    *task
}
