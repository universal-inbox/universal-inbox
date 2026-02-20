use std::str::FromStr;

use anyhow::{Context, anyhow};
use chrono::{DateTime, TimeZone, Utc};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use stripe::{
    BillingPortalSession, CheckoutSession, CheckoutSessionMode, Client, CreateBillingPortalSession,
    CreateCheckoutSession, CreateCheckoutSessionLineItems, CreateCustomer, Customer, CustomerId,
    Subscription, SubscriptionId, SubscriptionStatus as StripeSubscriptionStatus,
};
use url::Url;

use universal_inbox::{subscription::BillingInterval, user::User};

use crate::{configuration::StripeConfig, universal_inbox::UniversalInboxError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StripeSubscription {
    pub id: String,
    pub customer_id: String,
    pub status: StripeSubscriptionStatus,
    pub current_period_end: Option<DateTime<Utc>>,
    pub cancel_at_period_end: bool,
    pub billing_interval: Option<BillingInterval>,
}

impl TryFrom<Subscription> for StripeSubscription {
    type Error = UniversalInboxError;

    fn try_from(subscription: Subscription) -> Result<Self, Self::Error> {
        let billing_interval = subscription
            .items
            .data
            .first()
            .and_then(|item| item.price.as_ref())
            .and_then(|price| price.recurring.as_ref())
            .map(|recurring| match recurring.interval {
                stripe::RecurringInterval::Month => BillingInterval::Month,
                stripe::RecurringInterval::Year => BillingInterval::Year,
                _ => BillingInterval::Month,
            });

        Ok(StripeSubscription {
            id: subscription.id.to_string(),
            customer_id: subscription.customer.id().to_string(),
            status: subscription.status,
            current_period_end: Utc
                .timestamp_opt(subscription.current_period_end, 0)
                .single(),
            cancel_at_period_end: subscription.cancel_at_period_end,
            billing_interval,
        })
    }
}

#[derive(Clone)]
pub struct StripeService {
    client: Client,
    config: StripeConfig,
}

impl StripeService {
    pub fn new(config: StripeConfig) -> Result<Option<Self>, UniversalInboxError> {
        if !config.enabled {
            return Ok(None);
        }

        let secret_key = config
            .secret_key
            .as_ref()
            .ok_or_else(|| {
                UniversalInboxError::Unexpected(anyhow!(
                    "Stripe secret key is required when Stripe is enabled"
                ))
            })?
            .expose_secret()
            .0
            .clone();

        let client = Client::new(secret_key);

        Ok(Some(Self { client, config }))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = %user.id),
        err
    )]
    pub async fn create_customer(&self, user: &User) -> Result<String, UniversalInboxError> {
        let mut params = CreateCustomer::new();

        if let Some(ref email) = user.email {
            params.email = Some(email.as_str());
        }

        let full_name = user.full_name();
        if let Some(ref name) = full_name {
            params.name = Some(name.as_str());
        }

        let customer = Customer::create(&self.client, params)
            .await
            .context("Failed to create Stripe customer")?;

        Ok(customer.id.to_string())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            customer_id = %customer_id,
            price_id = %price_id
        ),
        err
    )]
    pub async fn create_checkout_session(
        &self,
        customer_id: &str,
        price_id: &str,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<Url, UniversalInboxError> {
        let customer_id = CustomerId::from_str(customer_id).map_err(|_| {
            UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Invalid customer ID: {customer_id}"),
            }
        })?;

        let mut params = CreateCheckoutSession::new();
        params.customer = Some(customer_id);
        params.mode = Some(CheckoutSessionMode::Subscription);
        params.success_url = Some(success_url);
        params.cancel_url = Some(cancel_url);
        params.line_items = Some(vec![CreateCheckoutSessionLineItems {
            price: Some(price_id.to_string()),
            quantity: Some(1),
            ..Default::default()
        }]);

        let session = CheckoutSession::create(&self.client, params)
            .await
            .context("Failed to create Stripe checkout session")?;

        let url_str = session.url.ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!(
                "Stripe checkout session created but URL is missing"
            ))
        })?;

        Url::parse(&url_str).map_err(|err| {
            UniversalInboxError::Unexpected(anyhow!(
                "Failed to parse Stripe checkout session URL `{url_str}`: {err}"
            ))
        })
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(customer_id = %customer_id),
        err
    )]
    pub async fn create_portal_session(
        &self,
        customer_id: &str,
        return_url: &str,
    ) -> Result<Url, UniversalInboxError> {
        let customer_id = CustomerId::from_str(customer_id).map_err(|_| {
            UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Invalid customer ID: {customer_id}"),
            }
        })?;

        let mut params = CreateBillingPortalSession::new(customer_id);
        params.return_url = Some(return_url);

        let session = BillingPortalSession::create(&self.client, params)
            .await
            .context("Failed to create Stripe billing portal session")?;

        Url::parse(&session.url).map_err(|err| {
            UniversalInboxError::Unexpected(anyhow!(
                "Failed to parse Stripe billing portal session URL `{}`: {err}",
                session.url
            ))
        })
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(subscription_id = %subscription_id),
        err
    )]
    pub async fn get_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<StripeSubscription, UniversalInboxError> {
        let subscription_id = SubscriptionId::from_str(subscription_id).map_err(|_| {
            UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Invalid subscription ID: {subscription_id}"),
            }
        })?;

        let subscription = Subscription::retrieve(&self.client, &subscription_id, &[])
            .await
            .map_err(|err| {
                if err.to_string().contains("No such subscription") {
                    UniversalInboxError::ItemNotFound(format!(
                        "Stripe subscription not found: {subscription_id}"
                    ))
                } else {
                    UniversalInboxError::Unexpected(anyhow!(
                        "Failed to retrieve Stripe subscription: {err}"
                    ))
                }
            })?;

        subscription.try_into()
    }

    pub fn price_id_monthly(&self) -> Option<&str> {
        self.config.price_id_monthly.as_deref()
    }

    pub fn price_id_annual(&self) -> Option<&str> {
        self.config.price_id_annual.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stripe_service_disabled() {
        let config = StripeConfig {
            enabled: false,
            ..Default::default()
        };

        let result = StripeService::new(config);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_stripe_service_missing_secret_key() {
        let config = StripeConfig {
            enabled: true,
            secret_key: None,
            ..Default::default()
        };

        let result = StripeService::new(config);
        assert!(result.is_err());
    }
}
