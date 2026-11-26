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

pub fn subscription_event(
    event_id: &str,
    event_type: &str,
    customer_id: &str,
    subscription_id: &str,
    status: &str,
    created: i64,
) -> String {
    serde_json::json!({
        "id": event_id,
        "object": "event",
        "api_version": "2025-03-31.basil",
        "created": created,
        "livemode": false,
        "pending_webhooks": 1,
        "type": event_type,
        "data": { "object": {
            "id": subscription_id,
            "object": "subscription",
            "automatic_tax": { "enabled": false },
            "billing_cycle_anchor": created,
            "billing_mode": { "type": "classic" },
            "cancel_at_period_end": false,
            "collection_method": "charge_automatically",
            "created": created,
            "currency": "eur",
            "customer": customer_id,
            "discounts": [],
            "invoice_settings": { "issuer": { "type": "self" } },
            "livemode": false,
            "metadata": {},
            "start_date": created,
            "status": status,
            "items": {
                "object": "list",
                "has_more": false,
                "url": format!("/v1/subscription_items?subscription={subscription_id}"),
                "data": [{
                    "id": "si_test123",
                    "object": "subscription_item",
                    "created": created,
                    "current_period_start": created,
                    "current_period_end": created + 1_000_000,
                    "discounts": [],
                    "metadata": {},
                    "subscription": subscription_id,
                    "plan": {
                        "id": "plan_test123", "object": "plan", "active": true, "amount": 900,
                        "billing_scheme": "per_unit", "created": created, "currency": "eur",
                        "interval": "month", "interval_count": 1, "livemode": false,
                        "usage_type": "licensed"
                    },
                    "price": {
                        "id": "price_test", "object": "price", "active": true,
                        "billing_scheme": "per_unit", "created": created, "currency": "eur",
                        "livemode": false, "metadata": {}, "product": "prod_test123",
                        "type": "recurring"
                    }
                }]
            }
        } }
    })
    .to_string()
}

pub fn stripe_signature(secret: &str, signed_at: i64, payload: &str) -> String {
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    let tag = ring::hmac::sign(&key, format!("{signed_at}.{payload}").as_bytes());
    format!("t={signed_at},v1={}", hex::encode(tag.as_ref()))
}
