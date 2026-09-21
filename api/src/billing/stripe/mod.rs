//! Stripe adapter (private to the billing subsystem).
//!
//! Wraps the `async-stripe` crate so that callers in
//! [`crate::billing::service`] never see Stripe types in public signatures.
//! All inputs / outputs across this module boundary are domain values defined
//! in `universal_inbox::billing` plus the local [`RawSubscription`] /
//! [`StripeEventKind`] enums declared here.

pub mod client;

pub use client::{
    CheckoutSessionParams, PortalSessionParams, RawSubscription, StripeClient, StripeError,
    StripeEvent, StripeEventKind,
};
