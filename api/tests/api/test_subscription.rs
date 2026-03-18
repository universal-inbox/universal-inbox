use chrono::{Duration, Utc};
use rstest::*;

use universal_inbox::subscription::{SubscriptionInfo, SubscriptionStatus, UserSubscription};

use crate::helpers::{
    TestedApp,
    auth::{authenticate_user, authenticated_app, AuthenticatedApp},
    subscription::{get_subscription_status, get_subscription_status_response},
    tested_app,
};

mod get_subscription_status {
    use pretty_assertions::assert_eq;
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_get_subscription_status_requires_authentication(
        #[future] tested_app: TestedApp,
    ) {
        let app = tested_app.await;
        let client = reqwest::Client::new();

        let response = get_subscription_status_response(&client, &app).await;

        assert_eq!(response.status(), 401);
    }

    #[rstest]
    #[tokio::test]
    async fn test_get_subscription_status_unlimited_when_stripe_disabled(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let AuthenticatedApp { client, app, .. } = authenticated_app.await;

        let subscription_info = get_subscription_status(&client, &app).await;

        // When Stripe is disabled, the service always returns Unlimited
        assert_eq!(subscription_info.status, SubscriptionStatus::Unlimited);
        assert!(!subscription_info.is_read_only);
        assert!(subscription_info.trial_ends_at.is_none());
        assert!(subscription_info.subscription_ends_at.is_none());
        assert!(subscription_info.days_remaining.is_none());
    }
}

/// Tests for subscription business logic via the service layer directly.
/// These test the subscription status logic that requires a subscription record
/// in the database, which is only used when Stripe is enabled. The service
/// methods are tested directly rather than through HTTP since the HTTP endpoint
/// short-circuits to `Unlimited` when Stripe is disabled in the test config.
mod subscription_service {
    use pretty_assertions::assert_eq;
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_trialing_subscription_is_not_read_only(#[future] tested_app: TestedApp) {
        let app = tested_app.await;
        let (_, user) =
            authenticate_user(&app, "user-trial", "Jane", "Doe", "jane@example.com").await;

        let trial_ends_at = Utc::now() + Duration::days(20);
        let subscription = UserSubscription::new_trial(user.id, trial_ends_at);

        let info = SubscriptionInfo::from_subscription(&subscription);

        assert_eq!(info.status, SubscriptionStatus::Trialing);
        assert!(!info.is_read_only);
        assert!(info.trial_ends_at.is_some());
        let days = info.days_remaining.unwrap();
        assert!(days >= 19 && days <= 20, "Expected ~20 days, got {days}");
    }

    #[rstest]
    #[tokio::test]
    async fn test_expired_trial_subscription_is_read_only(#[future] tested_app: TestedApp) {
        let app = tested_app.await;
        let (_, user) =
            authenticate_user(&app, "user-expired", "Bob", "Smith", "bob@example.com").await;

        let trial_ends_at = Utc::now() - Duration::days(5);
        let mut subscription = UserSubscription::new_trial(user.id, trial_ends_at);
        subscription.subscription_status = SubscriptionStatus::Expired;

        let info = SubscriptionInfo::from_subscription(&subscription);

        assert_eq!(info.status, SubscriptionStatus::Expired);
        assert!(info.is_read_only);
        assert!(info.days_remaining.is_none());
    }

    #[rstest]
    #[tokio::test]
    async fn test_start_trial_creates_unlimited_subscription_when_stripe_disabled(
        #[future] tested_app: TestedApp,
    ) {
        let app = tested_app.await;
        let (_, user) =
            authenticate_user(&app, "user-new", "Alice", "Brown", "alice@example.com").await;

        // When Stripe is disabled, start_trial creates an Unlimited subscription in DB
        let service = app.subscription_service.clone();
        let mut transaction = service.begin().await.unwrap();
        let subscription = service
            .get_subscription_by_user_id(&mut transaction, user.id)
            .await
            .unwrap();
        transaction.commit().await.unwrap();

        // User registration calls start_trial → should create an Unlimited subscription
        let subscription = subscription.expect("Subscription should exist after user registration");
        assert_eq!(subscription.subscription_status, SubscriptionStatus::Unlimited);
    }
}

mod create_checkout_session {
    use pretty_assertions::assert_eq;
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_create_checkout_session_requires_authentication(
        #[future] tested_app: TestedApp,
    ) {
        let app = tested_app.await;
        let client = reqwest::Client::new();

        let response = client
            .post(format!("{}subscriptions/checkout", app.api_address))
            .json(&serde_json::json!({
                "billing_interval": "monthly",
                "success_url": "https://example.com/success",
                "cancel_url": "https://example.com/cancel"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 401);
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_checkout_session_fails_when_stripe_disabled(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let AuthenticatedApp { client, app, .. } = authenticated_app.await;

        let response = client
            .post(format!("{}subscriptions/checkout", app.api_address))
            .json(&serde_json::json!({
                "billing_interval": "monthly",
                "success_url": "https://example.com/success",
                "cancel_url": "https://example.com/cancel"
            }))
            .send()
            .await
            .unwrap();

        // Stripe is disabled in test config → UnsupportedAction → 400
        assert_eq!(response.status(), 400);
    }
}

mod create_portal_session {
    use pretty_assertions::assert_eq;
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_create_portal_session_requires_authentication(#[future] tested_app: TestedApp) {
        let app = tested_app.await;
        let client = reqwest::Client::new();

        let response = client
            .post(format!("{}subscriptions/portal", app.api_address))
            .json(&serde_json::json!({
                "return_url": "https://example.com/settings"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 401);
    }

    #[rstest]
    #[tokio::test]
    async fn test_create_portal_session_fails_when_stripe_disabled(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let AuthenticatedApp { client, app, .. } = authenticated_app.await;

        let response = client
            .post(format!("{}subscriptions/portal", app.api_address))
            .json(&serde_json::json!({
                "return_url": "https://example.com/settings"
            }))
            .send()
            .await
            .unwrap();

        // Stripe is disabled in test config → UnsupportedAction → 400
        assert_eq!(response.status(), 400);
    }
}
