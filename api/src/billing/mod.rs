//! Server-side Stripe billing subsystem.
//!
//! The whole tree is gated behind the optional `[billing]` configuration block:
//! when absent, none of the code here is wired up and Universal Inbox runs as
//! it does for self-hosters today. When present, this module supplies:
//!
//! - [`repository::BillingRepository`] — persistence for `user_subscription`
//!   and `stripe_event` rows.
//! - [`stripe`] — `async-stripe` adapter. All Stripe SDK types are confined
//!   here; downstream code consumes the [`stripe::StripeClient`] trait.

pub mod commands;
pub mod repository;
pub mod routes;
pub mod service;
pub mod stripe;
