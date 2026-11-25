use super::TestedApp;

/// `stripe_event.received_at` is set by the database and has no repository setter.
pub async fn backdate_stripe_event(app: &TestedApp, event_id: &str, days: i64) {
    let mut transaction = app.repository.begin().await.unwrap();
    sqlx::query(
        "UPDATE stripe_event SET received_at = now() - make_interval(days => $1::int) WHERE event_id = $2",
    )
    .bind(days)
    .bind(event_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}
