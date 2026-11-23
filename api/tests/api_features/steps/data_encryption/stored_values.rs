use pretty_assertions::assert_eq;

use super::*;

async fn store_todoist_item(world: &mut ApiWorld, item: TodoistItem) {
    let user_id = world.user().id;
    let data = ThirdPartyItemData::TodoistItem(Box::new(item));
    let app = world.app.get().await;
    let created = create_task_third_party_item(app, data.clone(), user_id).await;
    world.encryption.stored_item = Some(StoredItem {
        item: created.third_party_item,
        data,
        task: created.task,
    });
}

#[when("a Todoist task whose content contains the secret marker is stored")]
async fn a_todoist_task_with_a_secret_content_is_stored(world: &mut ApiWorld) {
    store_todoist_item(
        world,
        TodoistItem {
            project_id: "1111".to_string(),
            content: format!("Task {SECRET_MARKER}"),
            ..*todoist_item()
        },
    )
    .await;
}

#[when("a Todoist task whose description contains the secret marker is stored")]
async fn a_todoist_task_with_a_secret_description_is_stored(world: &mut ApiWorld) {
    let body = format!("Description {SECRET_MARKER}");
    world.encryption.expected_task_body = Some(body.clone());
    store_todoist_item(
        world,
        TodoistItem {
            project_id: "1111".to_string(),
            description: body,
            ..*todoist_item()
        },
    )
    .await;
}

fn stored_item(world: &ApiWorld) -> &StoredItem {
    world
        .encryption
        .stored_item
        .as_ref()
        .expect("No item was stored")
}

#[then("the stored item data does not contain the secret marker")]
async fn the_stored_item_data_does_not_contain_the_marker(world: &mut ApiWorld) {
    let id = stored_item(world).item.id.0;
    let app = world.app.get().await;
    let data_enc: Vec<u8> =
        sqlx::query_scalar("SELECT data_enc FROM third_party_item WHERE id = $1")
            .bind(id)
            .fetch_one(&*app.repository.pool)
            .await
            .unwrap();
    assert!(!contains(&data_enc, SECRET_MARKER));
}

#[then(expr = "the data encryption status counts {int} third-party item value(s) on the first key")]
async fn the_status_counts_items_on_the_first_key(world: &mut ApiWorld, count: i64) {
    let app = world.app.get().await;
    let status = data_encryption_service(app, current_keyring())
        .status()
        .await
        .unwrap();
    assert_eq!(
        values_on_key(&status, "third_party_item.data_enc", "1"),
        count
    );
}

#[then("reading the item back gives the data that was stored")]
async fn reading_the_item_back_gives_the_data(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let (source_id, expected) = {
        let stored = stored_item(world);
        (stored.item.source_id.clone(), stored.data.clone())
    };
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let items = app
        .repository
        .find_third_party_items_for_source_id(
            &mut transaction,
            ThirdPartyItemKind::TodoistItem,
            &source_id,
            Some(user_id),
        )
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].data, expected);
}

#[then("the task body is the description")]
fn the_task_body_is_the_description(world: &mut ApiWorld) {
    let task = stored_item(world)
        .task
        .as_ref()
        .expect("No task was created");
    assert_eq!(&task.body, expected_task_body(world));
}

fn expected_task_body(world: &ApiWorld) -> &String {
    world
        .encryption
        .expected_task_body
        .as_ref()
        .expect("No task description was stored")
}

#[then("the stored task body is encrypted and absent from the search vector")]
async fn the_stored_task_body_is_encrypted(world: &mut ApiWorld) {
    let task_id = stored_item(world).task.as_ref().unwrap().id.0;
    let app = world.app.get().await;
    let row =
        sqlx::query("SELECT body_enc, title_project_tags_tsv::TEXT AS tsv FROM task WHERE id = $1")
            .bind(task_id)
            .fetch_one(&*app.repository.pool)
            .await
            .unwrap();
    assert!(!contains(&row.get::<Vec<u8>, _>("body_enc"), SECRET_MARKER));
    assert!(!row.get::<String, _>("tsv").contains("marker"));
}

#[then("reading the task back gives the same body")]
async fn reading_the_task_back_gives_the_same_body(world: &mut ApiWorld) {
    let task_id = stored_item(world).task.as_ref().unwrap().id;
    let expected = expected_task_body(world).clone();
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let stored_task = app
        .repository
        .get_one_task(&mut transaction, task_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_task.body, expected);
}

#[when("an OAuth credential whose raw token response holds the secret marker is stored")]
async fn an_oauth_credential_with_a_secret_response_is_stored(world: &mut ApiWorld) {
    let connection_id = world
        .integrations
        .todoist_connection
        .as_ref()
        .expect("The account is not connected to Todoist")
        .id;
    let raw_token_response = serde_json::json!({ "authed_user": { "id": SECRET_MARKER } });
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let stored = app
        .repository
        .store_oauth_credential(
            &mut transaction,
            connection_id,
            vec![1, 2, 3],
            None,
            None,
            raw_token_response.clone(),
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.encryption.raw_token_response = Some(RawTokenResponse {
        given: raw_token_response,
        reported: stored.raw_token_response,
    });
}

#[then("the stored credential reports the raw token response it was given")]
fn the_stored_credential_reports_the_response(world: &mut ApiWorld) {
    let response = world.encryption.raw_token_response.as_ref().unwrap();
    assert_eq!(response.reported, response.given);
}

#[then("the raw token response is stored encrypted")]
async fn the_raw_token_response_is_stored_encrypted(world: &mut ApiWorld) {
    let connection_id = todoist_connection_id(world);
    let app = world.app.get().await;
    let encrypted: Vec<u8> = sqlx::query_scalar(
        "SELECT raw_token_response_enc FROM oauth_credential WHERE integration_connection_id = $1",
    )
    .bind(connection_id)
    .fetch_one(&*app.repository.pool)
    .await
    .unwrap();
    assert!(!contains(&encrypted, SECRET_MARKER));
}

#[then("reading the credential back gives the same raw token response")]
async fn reading_the_credential_back(world: &mut ApiWorld) {
    let connection_id = world.integrations.todoist_connection.as_ref().unwrap().id;
    let expected = world
        .encryption
        .raw_token_response
        .as_ref()
        .unwrap()
        .given
        .clone();
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let fetched = app
        .repository
        .get_oauth_credential(&mut transaction, connection_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.raw_token_response, expected);
}
