use std::collections::HashMap;

use rstest::*;
use uuid::Uuid;

use universal_inbox::{SuccessResponse, user::UserId};

use universal_inbox_api::configuration::Settings;

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
    settings, tested_app_with_local_auth,
    user::{get_current_user, get_current_user_response},
};

/// The unauthenticated passkey start endpoints had two enumeration oracles:
///
///   * `POST /users/passkeys/registration/start` returned
///     `AlreadyExists { id: user_id.0 }` when the supplied username was
///     already registered, leaking the existing account's `UserId` (UUID)
///     to any unauthenticated caller in the `409 Conflict` body.
///
///   * `POST /users/passkeys/authentication/start` returned a `400 Bad
///     Request` with `ItemNotFound: No user found for username <X>` for
///     unknown usernames, while known usernames received a `200 OK` with
///     a `RequestChallengeResponse`, allowing username/email enumeration
///     by status-code probing.
///
/// The fix synthesizes shape-identical responses regardless of whether
/// the username exists: registration always returns a fresh ephemeral
/// `UserId`-bound challenge, and authentication mints a discoverable-
/// style challenge. `allow_credentials` is cleared for known and unknown
/// usernames alike, so neither the account's existence nor its credential
/// IDs reach the client (the real credential stays in the server-side
/// ceremony state). The HTTP status and body envelope are identical, and
/// no `user_id` or username text reaches the client on the "exists" or
/// "missing" paths.
mod passkey_start_non_enumerable {
    use super::*;
    use pretty_assertions::assert_eq;
    use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
    use webauthn_rs::prelude::{CreationChallengeResponse, RequestChallengeResponse};

    use crate::helpers::user::{
        finish_passkey_registration_response_unauthenticated,
        start_passkey_authentication_response, start_passkey_registration_response,
    };

    /// Drive the full unauthenticated passkey registration ceremony so a
    /// known passkey user exists in the database. Returns the registered
    /// credential ID (base64url).
    async fn register_passkey_user(app: &TestedApp, username: &str) -> String {
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let start_response = start_passkey_registration_response(&client, app, username).await;
        assert_eq!(start_response.status(), reqwest::StatusCode::OK);
        let body = start_response.text().await.unwrap();
        let (creation_challenge, nonce) = crate::helpers::user::split_creation_challenge(&body);

        let origin = app.front_base_url.clone();
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let register_credential = authenticator
            .do_registration(origin, creation_challenge)
            .expect("Failed to complete passkey registration with software authenticator");

        let finish_response = finish_passkey_registration_response_unauthenticated(
            &client,
            app,
            &register_credential,
            &nonce,
        )
        .await;
        assert_eq!(finish_response.status(), reqwest::StatusCode::OK);
        register_credential.id
    }

    /// Replace the per-ceremony values (challenge, nonce) so two start
    /// responses can be compared structurally.
    fn redact_ceremony_values(body: &str) -> serde_json::Value {
        let mut value: serde_json::Value = serde_json::from_str(body).unwrap();
        value["nonce"] = "<redacted>".into();
        value["publicKey"]["challenge"] = "<redacted>".into();
        value
    }

    #[rstest]
    #[tokio::test]
    async fn test_start_passkey_registration_is_non_enumerable(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let registered_username = "alice_existing_passkey";
        register_passkey_user(&app, registered_username).await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        // Probe with a fresh username — the baseline success path.
        let fresh_response =
            start_passkey_registration_response(&client, &app, "bob_fresh_passkey").await;
        let fresh_status = fresh_response.status();
        assert_eq!(
            fresh_status,
            reqwest::StatusCode::OK,
            "fresh username must succeed"
        );
        let fresh_body = fresh_response.text().await.unwrap();
        let fresh_challenge: CreationChallengeResponse = serde_json::from_str(&fresh_body)
            .expect("fresh response must be a CreationChallengeResponse");

        // Probe with the same username we just registered. The fix MUST
        // return a CreationChallengeResponse with HTTP 200, not a 409
        // Conflict with a leaked UserId UUID.
        let exists_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        let exists_response =
            start_passkey_registration_response(&exists_client, &app, registered_username).await;
        let exists_status = exists_response.status();

        assert_eq!(
            exists_status, fresh_status,
            "existing-username response status must match fresh-username status",
        );
        let exists_body = exists_response.text().await.unwrap();
        let exists_challenge: CreationChallengeResponse = serde_json::from_str(&exists_body)
            .expect(
                "existing-username response must also be a CreationChallengeResponse, not an \
                 AlreadyExists error envelope",
            );

        // The two responses must be structurally identical: same RP id,
        // same user.name, same user.displayName. Only the challenge bytes
        // and the synthetic user.id are allowed to differ (the user.id
        // for the registered case MUST be a fresh ephemeral UUID and
        // never the real account's UserId).
        assert_eq!(
            fresh_challenge.public_key.rp.id, exists_challenge.public_key.rp.id,
            "RP id must be identical"
        );
        assert_eq!(
            fresh_challenge.public_key.user.name, "bob_fresh_passkey",
            "fresh response user.name should echo the supplied username"
        );
        assert_eq!(
            exists_challenge.public_key.user.name, registered_username,
            "existing-username response should also echo the supplied username (parity)"
        );

        // No UUID may appear in the existing-username body except the
        // synthetic ephemeral user.id inside the CreationChallengeResponse.
        // In particular, no `AlreadyExists` message form ("The entity X
        // already exists") may be present.
        assert!(
            !exists_body.contains("already exists"),
            "existing-username response leaks an AlreadyExists error envelope: {exists_body}",
        );
        assert!(
            !exists_body.contains("AlreadyExists"),
            "existing-username response leaks an AlreadyExists variant name: {exists_body}",
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_start_passkey_authentication_is_non_enumerable(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let registered_username = "carol_registered_passkey";
        let credential_id = register_passkey_user(&app, registered_username).await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        // Known username — baseline success path.
        let known_response =
            start_passkey_authentication_response(&client, &app, registered_username).await;
        let known_status = known_response.status();
        assert_eq!(
            known_status,
            reqwest::StatusCode::OK,
            "known username must succeed with 200 OK"
        );
        let known_body = known_response.text().await.unwrap();
        let known_challenge: RequestChallengeResponse = serde_json::from_str(&known_body)
            .expect("known-username response must be a RequestChallengeResponse");

        // Unknown username — pre-fix this returned 400 ItemNotFound with
        // the username echoed. The fix MUST produce a 200 OK
        // RequestChallengeResponse instead.
        let unknown_username = "ghost_not_registered_passkey";
        let unknown_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        let unknown_response =
            start_passkey_authentication_response(&unknown_client, &app, unknown_username).await;
        let unknown_status = unknown_response.status();

        assert_eq!(
            unknown_status, known_status,
            "unknown-username status must match known-username status",
        );
        let unknown_body = unknown_response.text().await.unwrap();
        let unknown_challenge: RequestChallengeResponse = serde_json::from_str(&unknown_body)
            .expect(
                "unknown-username response must be a RequestChallengeResponse, not an \
                 ItemNotFound error envelope",
            );

        // The unknown-username body MUST NOT echo the supplied username,
        // and MUST NOT carry an ItemNotFound message form.
        assert!(
            !unknown_body.contains(unknown_username),
            "unknown-username response leaks the supplied username: {unknown_body}",
        );
        assert!(
            !unknown_body.contains("Item not found"),
            "unknown-username response leaks an ItemNotFound envelope: {unknown_body}",
        );
        assert!(
            !unknown_body.contains("ItemNotFound"),
            "unknown-username response leaks an ItemNotFound variant name: {unknown_body}",
        );

        // RP id parity — the field that matters most for shape parity.
        assert_eq!(
            known_challenge.public_key.rp_id, unknown_challenge.public_key.rp_id,
            "RP id must be identical across known/unknown branches",
        );

        // No credential ID may reach an unauthenticated caller: the
        // allow-list is empty in both branches.
        assert!(
            known_challenge.public_key.allow_credentials.is_empty(),
            "known-username response exposes allow_credentials: {known_body}",
        );
        assert!(
            unknown_challenge.public_key.allow_credentials.is_empty(),
            "unknown-username response exposes allow_credentials: {unknown_body}",
        );
        assert!(
            !known_body.contains(&credential_id),
            "known-username response leaks the registered credential ID: {known_body}",
        );

        // Same JSON structure once the per-ceremony values are redacted.
        assert_eq!(
            redact_ceremony_values(&known_body),
            redact_ceremony_values(&unknown_body),
            "known and unknown start responses must be structurally identical",
        );
    }
}

/// The passkey authentication start endpoint is unauthenticated. Its Redis
/// state used to be keyed on the resolved user id, so a second start for
/// the same username (from anyone) replaced the state of the victim's
/// in-flight ceremony and their finish call failed. The state is now keyed
/// on a per-ceremony id held in the caller's session.
mod passkey_authentication_ceremony_isolation {
    use super::*;
    use serde_json::Value;
    use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
    use webauthn_rs::prelude::RequestChallengeResponse;

    use crate::helpers::user::{
        finish_body_with_nonce, finish_passkey_registration_response_unauthenticated,
        front_origin_header, split_creation_challenge, start_passkey_authentication_response,
        start_passkey_registration_response,
    };

    fn split_request_challenge(body: &str) -> (RequestChallengeResponse, String) {
        let value: Value = serde_json::from_str(body).unwrap();
        let nonce = value["nonce"].as_str().unwrap().to_string();
        (serde_json::from_str(body).unwrap(), nonce)
    }

    fn cookie_client() -> reqwest::Client {
        reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap()
    }

    #[rstest]
    #[tokio::test]
    async fn test_concurrent_start_does_not_clobber_an_in_flight_ceremony(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;
        let username = "dave_passkey_victim";
        let origin = app.front_base_url.clone();
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));

        // Register the victim's passkey
        let registration_client = cookie_client();
        let start_response =
            start_passkey_registration_response(&registration_client, &app, username).await;
        let (creation_challenge, nonce) =
            split_creation_challenge(&start_response.text().await.unwrap());
        let register_credential = authenticator
            .do_registration(origin.clone(), creation_challenge)
            .unwrap();
        let finish_response = finish_passkey_registration_response_unauthenticated(
            &registration_client,
            &app,
            &register_credential,
            &nonce,
        )
        .await;
        assert_eq!(finish_response.status(), reqwest::StatusCode::OK);

        // The victim starts logging in...
        let victim = cookie_client();
        let start_response = start_passkey_authentication_response(&victim, &app, username).await;
        assert_eq!(start_response.status(), reqwest::StatusCode::OK);
        let (request_challenge, victim_nonce) =
            split_request_challenge(&start_response.text().await.unwrap());

        // ...while an attacker starts a ceremony for the same username.
        let attacker = cookie_client();
        let attacker_start = start_passkey_authentication_response(&attacker, &app, username).await;
        assert_eq!(attacker_start.status(), reqwest::StatusCode::OK);

        // The server no longer sends credential IDs. SoftPasskey is not a
        // discoverable authenticator, so emulate one: it looks up its own
        // resident credential and presents it as the allow-list entry.
        let mut request_challenge = request_challenge;
        assert!(request_challenge.public_key.allow_credentials.is_empty());
        request_challenge.public_key.allow_credentials = serde_json::from_value(
            serde_json::json!([{ "type": "public-key", "id": register_credential.raw_id }]),
        )
        .unwrap();

        // The victim's ceremony still completes.
        let credential = authenticator
            .do_authentication(origin, request_challenge)
            .unwrap();
        let response = victim
            .post(format!(
                "{}users/passkeys/authentication/finish",
                app.api_address
            ))
            .header(reqwest::header::ORIGIN, front_origin_header(&app))
            .json(&finish_body_with_nonce(&credential, &victim_nonce))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }
}

/// `POST /users/passkeys/registration/finish` previously surfaced the
/// `user_auth_username_key` unique-constraint violation as a raw
/// `DatabaseError`. The HTTP body was:
///
/// ```json
/// {"message":"Database error: Failed to create user auth for user <UUID>: \
///   error returned from database: duplicate key value violates unique \
///   constraint \"user_auth_username_key\""}
/// ```
///
/// That body leaked the Postgres constraint name (a direct username-
/// collision signal), the raw sqlx error text, and the ephemeral
/// `UserId` UUID minted during the start ceremony. The fix maps the
/// `23505` on `user_auth_username_key` in `Repository::create_user_auth`
/// to a `UniversalInboxError::Conflict` with a generic user-facing
/// message, producing a `409 Conflict` body that carries no internal
/// implementation details.
mod passkey_finish_non_leaking {
    use super::*;
    use pretty_assertions::assert_eq;
    use regex::Regex;
    use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};

    use crate::helpers::user::{
        finish_passkey_registration_response_unauthenticated, split_creation_challenge,
        start_passkey_registration_response,
    };

    #[rstest]
    #[tokio::test]
    async fn test_finish_passkey_registration_with_taken_username_does_not_leak(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        // 1. Register a passkey user so the username is taken.
        let registered_username = "alice_taken_username";
        register_passkey_user(&app, registered_username).await;

        // 2. Drive a second start ceremony for the same username with a
        //    fresh client (no shared session/cookie state) and a fresh
        //    SoftPasskey. The start endpoint is already non-enumerable
        //    (covered by `passkey_start_non_enumerable`); it returns 200
        //    with a CreationChallengeResponse bound to an ephemeral
        //    UserId. We need that challenge to drive the authenticator.
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let start_response =
            start_passkey_registration_response(&client, &app, registered_username).await;
        assert_eq!(
            start_response.status(),
            reqwest::StatusCode::OK,
            "start endpoint must remain non-enumerable for taken username"
        );
        let (creation_challenge, nonce) =
            split_creation_challenge(&start_response.text().await.unwrap());

        let origin = app.front_base_url.clone();
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let register_credential = authenticator
            .do_registration(origin, creation_challenge)
            .expect("Failed to complete passkey registration with software authenticator");

        // 3. Call finish; the unique-constraint violation must surface as
        //    a clean 409 Conflict, not a raw DatabaseError.
        let finish_response = finish_passkey_registration_response_unauthenticated(
            &client,
            &app,
            &register_credential,
            &nonce,
        )
        .await;
        let finish_status = finish_response.status();
        let finish_body = finish_response.text().await.unwrap();

        assert_eq!(
            finish_status,
            reqwest::StatusCode::CONFLICT,
            "duplicate-username finish must be 409 Conflict, body was: {finish_body}",
        );

        let parsed: serde_json::Value =
            serde_json::from_str(&finish_body).expect("finish response body must parse as JSON");
        let message = parsed
            .get("message")
            .and_then(|v| v.as_str())
            .expect("finish response body must have a string `message` field");
        assert_eq!(
            message, "This username is already taken. Please choose a different one.",
            "finish response message must match the generic user-facing copy verbatim",
        );

        // 4. The body must not leak any internal implementation details.
        let forbidden_substrings = [
            "user_auth_username_key",
            "duplicate key",
            "Database error",
            "DatabaseError",
            "unique constraint",
            "AlreadyExists",
            "sqlx",
            registered_username,
        ];
        for needle in &forbidden_substrings {
            assert!(
                !finish_body.contains(needle),
                "finish response leaks {needle:?}: {finish_body}",
            );
        }

        // No UUID may appear in the response body. The ephemeral UserId
        // minted at start time is the most sensitive value to suppress
        // here, but a stricter check that no UUID-shaped substring appears
        // also catches the unlikely case that some other id leaks.
        let uuid_regex = Regex::new(
            r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
        )
        .unwrap();
        assert!(
            !uuid_regex.is_match(&finish_body),
            "finish response leaks a UUID: {finish_body}",
        );
    }

    /// Drive the full unauthenticated passkey registration ceremony so a
    /// known passkey user exists in the database. Duplicated from
    /// `passkey_start_non_enumerable` to keep the modules independent.
    async fn register_passkey_user(app: &TestedApp, username: &str) {
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let start_response = start_passkey_registration_response(&client, app, username).await;
        assert_eq!(start_response.status(), reqwest::StatusCode::OK);
        let (creation_challenge, nonce) =
            split_creation_challenge(&start_response.text().await.unwrap());

        let origin = app.front_base_url.clone();
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let register_credential = authenticator
            .do_registration(origin, creation_challenge)
            .expect("Failed to complete passkey registration with software authenticator");

        let finish_response = finish_passkey_registration_response_unauthenticated(
            &client,
            app,
            &register_credential,
            &nonce,
        )
        .await;
        assert_eq!(finish_response.status(), reqwest::StatusCode::OK);
    }
}

/// Regression tests for universal-inbox-bkj.32: per-ceremony nonce
/// binding, Origin/Referer check, and cross-flow Redis-prefix isolation
/// for the four passkey start/finish handlers.
///
/// The pre-fix passkey ceremony handlers relied solely on the cookie
/// session + a Redis state blob keyed by `user_id`. There was no
/// per-flow secret the frontend had to echo back, no Origin/Referer
/// check, and a tampered or cross-flow session could in principle pair
/// with state from another flow. The handlers now:
///
///   - generate a fresh per-ceremony nonce at start, return it to the
///     client, and require the client to echo it on finish;
///   - reject any request whose `Origin` (or `Referer`) does not match
///     the configured `application.front_base_url`;
///   - keep the ADD-passkey-to-existing-user flow and the
///     INITIAL-passkey-registration flow in disjoint session/Redis key
///     namespaces so they can never be cross-consumed.
mod passkey_ceremony_csrf_hardening {
    use super::*;
    use pretty_assertions::assert_eq;
    use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};
    use webauthn_rs::prelude::CreationChallengeResponse;

    use crate::helpers::user::{
        finish_add_passkey_registration_response,
        finish_passkey_registration_response_unauthenticated, front_origin_header, register_user,
        split_creation_challenge, start_add_passkey_registration_response,
        start_passkey_registration_response,
    };

    /// Verify that the start response includes a `nonce` and that a
    /// finish call with a *mismatched* nonce is rejected, while the
    /// matching nonce succeeds.
    #[rstest]
    #[tokio::test]
    async fn test_finish_passkey_registration_requires_matching_nonce(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        // Start the unauthenticated passkey registration flow.
        let start_response =
            start_passkey_registration_response(&client, &app, "nonce_match_alice").await;
        assert_eq!(start_response.status(), reqwest::StatusCode::OK);
        let start_body = start_response.text().await.unwrap();
        let (creation_challenge, server_nonce) = split_creation_challenge(&start_body);

        // The start response MUST include a non-empty nonce.
        assert!(
            !server_nonce.is_empty(),
            "start response must include a non-empty nonce"
        );

        // Complete the WebAuthn challenge so we have a valid credential
        // payload to send to finish.
        let origin = app.front_base_url.clone();
        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let register_credential = authenticator
            .do_registration(origin, creation_challenge)
            .expect("Failed to complete passkey registration");

        // First attempt: a deliberately mismatched nonce must be
        // rejected (401 per the existing Unauthorized convention used
        // for session-bound CSRF rejection).
        let bogus_nonce = "AAAAAAAAAAAAAAAAAAAAAA";
        assert_ne!(
            bogus_nonce, server_nonce,
            "test pre-condition: the bogus nonce must differ from the server-issued one"
        );
        let bad_response = finish_passkey_registration_response_unauthenticated(
            &client,
            &app,
            &register_credential,
            bogus_nonce,
        )
        .await;
        assert_eq!(
            bad_response.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "finish with mismatched nonce must be rejected"
        );

        // Second attempt: even with the wrong nonce above, the start
        // state was already consumed (the finish handler `get_del`s the
        // Redis blob on first read), so we cannot re-use the same
        // ceremony with the correct nonce — drive a fresh ceremony and
        // verify the happy path with the right nonce.
        let fresh_client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();
        let start2 =
            start_passkey_registration_response(&fresh_client, &app, "nonce_match_alice_v2").await;
        assert_eq!(start2.status(), reqwest::StatusCode::OK);
        let (challenge2, good_nonce) = split_creation_challenge(&start2.text().await.unwrap());
        let mut auth2 = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let cred2 = auth2
            .do_registration(app.front_base_url.clone(), challenge2)
            .expect("Failed to complete passkey registration");
        let good_response = finish_passkey_registration_response_unauthenticated(
            &fresh_client,
            &app,
            &cred2,
            &good_nonce,
        )
        .await;
        assert_eq!(
            good_response.status(),
            reqwest::StatusCode::OK,
            "finish with the server-issued nonce must succeed"
        );
    }

    /// A finish call carrying an `Origin` header pointing at a
    /// foreign origin must be rejected; the matching origin still
    /// succeeds.
    #[rstest]
    #[tokio::test]
    async fn test_finish_passkey_registration_rejects_foreign_origin(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .unwrap();

        let start_response =
            start_passkey_registration_response(&client, &app, "origin_check_bob").await;
        assert_eq!(start_response.status(), reqwest::StatusCode::OK);
        let (challenge, nonce) = split_creation_challenge(&start_response.text().await.unwrap());

        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let credential = authenticator
            .do_registration(app.front_base_url.clone(), challenge)
            .expect("Failed to complete passkey registration");

        // Build a finish call by hand so we can swap the Origin header.
        let body = crate::helpers::user::finish_body_with_nonce(&credential, &nonce);
        let evil_response = client
            .post(format!(
                "{}users/passkeys/registration/finish",
                app.api_address
            ))
            .header(reqwest::header::ORIGIN, "https://evil.example")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            evil_response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "finish with foreign Origin must be rejected with 400"
        );

        // The state is still in place (the foreign-origin call was
        // rejected before any state consumption). Retry from the same
        // client with the correct Origin header and the same nonce —
        // it must succeed.
        let ok_response = client
            .post(format!(
                "{}users/passkeys/registration/finish",
                app.api_address
            ))
            .header(reqwest::header::ORIGIN, front_origin_header(&app))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            ok_response.status(),
            reqwest::StatusCode::OK,
            "finish with the configured front-end Origin must succeed"
        );
    }

    /// Cross-flow / cross-prefix isolation: the ADD-passkey-to-existing
    /// -user flow (authenticated, `/users/me/auth-methods/passkey/...`)
    /// uses session key `add-passkey-registration-state` and Redis
    /// prefix `add-passkey-registration-state::{user_id}`. The
    /// INITIAL-passkey-registration flow (unauthenticated,
    /// `/users/passkeys/...`) uses key `passkey-registration-state`.
    /// Starting an ADD flow MUST NOT leave state that the INITIAL
    /// finish handler can consume, even when both flows would (pre-fix)
    /// have keyed Redis by the same `user_id`.
    ///
    /// The session-key separation alone is sufficient to demonstrate
    /// the isolation: the INITIAL finish handler reads
    /// `passkey-registration-state` from the session and finds nothing
    /// when only the ADD flow's `add-passkey-registration-state` was
    /// set.
    #[rstest]
    #[tokio::test]
    async fn test_add_passkey_state_cannot_complete_initial_passkey_finish(
        #[future] tested_app_with_local_auth: TestedApp,
    ) {
        let app = tested_app_with_local_auth.await;

        // Register a local user and authenticate the client.
        let (client, _user) = register_user(
            &app,
            "carol@doe.name".parse().unwrap(),
            "Very-harD-pasSword-5",
        )
        .await;

        // Start the ADD-passkey-to-existing-user flow — this sets up
        // the ADD-flow session key and Redis blob.
        let start_response =
            start_add_passkey_registration_response(&client, &app, "carol_passkey").await;
        assert_eq!(start_response.status(), reqwest::StatusCode::OK);
        let (creation_challenge, add_nonce): (CreationChallengeResponse, String) =
            split_creation_challenge(&start_response.text().await.unwrap());

        let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
        let register_credential = authenticator
            .do_registration(app.front_base_url.clone(), creation_challenge)
            .expect("Failed to complete passkey registration");

        // Crossover attempt: try to consume the ADD-flow state via the
        // INITIAL passkey finish endpoint. The INITIAL endpoint reads
        // `passkey-registration-state` from the session, which was
        // never written, so the call must be rejected. (Before the
        // session-key split + nonce binding, an attacker who could
        // confuse the routing by user_id might have driven state from
        // one flow into the other; the current handlers cannot.)
        let cross_flow_body =
            crate::helpers::user::finish_body_with_nonce(&register_credential, &add_nonce);
        let cross_response = client
            .post(format!(
                "{}users/passkeys/registration/finish",
                app.api_address
            ))
            .header(reqwest::header::ORIGIN, front_origin_header(&app))
            .json(&cross_flow_body)
            .send()
            .await
            .unwrap();
        assert!(
            !cross_response.status().is_success(),
            "ADD-flow state must not be consumable via the INITIAL finish endpoint, but \
             got {}",
            cross_response.status()
        );

        // Sanity check: the proper ADD finish endpoint, with the same
        // credential and the correct nonce, must succeed — proving the
        // ADD state itself is intact and only the cross-flow attempt
        // above was blocked by the session-key namespacing.
        let proper_response = finish_add_passkey_registration_response(
            &client,
            &app,
            &register_credential,
            &add_nonce,
        )
        .await;
        assert_eq!(
            proper_response.status(),
            reqwest::StatusCode::OK,
            "ADD finish with the correct endpoint + nonce must succeed"
        );
    }
}

mod delete_user {
    use super::*;
    use pretty_assertions::assert_eq;

    use chrono::Utc;
    use slack_morphism::prelude::{SlackChannelId, SlackTeamId, SlackTs};
    use universal_inbox::{
        integration_connection::{
            config::IntegrationConnectionConfig, integrations::github::GithubConfig,
        },
        slack_bridge::{SlackBridgeActionStatus, SlackBridgeActionType, SlackBridgePendingAction},
        third_party::integrations::github::GithubNotification,
    };
    use universal_inbox_api::repository::slack_bridge::SlackBridgeRepository;

    use crate::helpers::{
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection, github_oauth_credential,
        },
        notification::github::{create_notification_from_github_notification, github_notification},
        tested_app,
        user::delete_current_user_response,
    };

    /// Every `(table, column)` holding a foreign key to `"user"`, read from the
    /// live schema so a table added later is covered without touching this
    /// test.
    async fn user_foreign_keys(app: &TestedApp) -> Vec<(String, String)> {
        sqlx::query_as::<_, (String, String)>(
            r#"
                SELECT c.conrelid::regclass::text, a.attname::text
                FROM pg_constraint c
                JOIN pg_attribute a
                  ON a.attrelid = c.conrelid AND a.attnum = ANY (c.conkey)
                WHERE c.contype = 'f' AND c.confrelid = '"user"'::regclass
            "#,
        )
        .fetch_all(&*app.repository.pool)
        .await
        .unwrap()
    }

    async fn count_rows(app: &TestedApp, table: &str, column: &str, id: Uuid) -> i64 {
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE {column} = $1"
        )))
        .bind(id)
        .fetch_one(&*app.repository.pool)
        .await
        .unwrap()
    }

    async fn rows_owned_by(app: &TestedApp, user_id: UserId) -> HashMap<String, i64> {
        let mut counts = HashMap::new();
        for (table, column) in user_foreign_keys(app).await {
            let count = count_rows(app, &table, &column, user_id.0).await;
            counts.insert(format!("{table}.{column}"), count);
        }
        counts.insert(
            "user.id".to_string(),
            count_rows(app, "\"user\"", "id", user_id.0).await,
        );
        counts
    }

    /// Give the user data in as many owned tables as practical.
    async fn seed_user_data(
        app: &AuthenticatedApp,
        settings: &Settings,
        github_notification: &GithubNotification,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let response = app
            .client
            .post(format!(
                "{}users/me/authentication-tokens",
                app.app.api_address
            ))
            .header(
                reqwest::header::ORIGIN,
                crate::helpers::user::front_origin_header(&app.app),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let github_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Github(GithubConfig::enabled()),
            settings,
            github_oauth_credential,
            None,
            None,
        )
        .await;
        let notification = create_notification_from_github_notification(
            &app.app,
            github_notification,
            app.user.id,
            github_integration_connection.id,
        )
        .await;

        let now = Utc::now();
        let mut transaction = app.app.repository.begin().await.unwrap();
        app.app
            .repository
            .create_pending_action(
                &mut transaction,
                &SlackBridgePendingAction {
                    id: Uuid::new_v4().into(),
                    user_id: app.user.id,
                    notification_id: Some(notification.id),
                    action_type: SlackBridgeActionType::MarkAsRead,
                    slack_team_id: SlackTeamId::new("T1234".to_string()),
                    slack_channel_id: SlackChannelId::new("C1234".to_string()),
                    slack_thread_ts: SlackTs::new("1700000000.000100".to_string()),
                    slack_last_message_ts: SlackTs::new("1700000000.000200".to_string()),
                    status: SlackBridgeActionStatus::Pending,
                    failure_message: None,
                    retry_count: 0,
                    created_at: now,
                    updated_at: now,
                    completed_at: None,
                },
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();
    }

    #[rstest]
    #[tokio::test]
    async fn test_user_can_delete_their_account(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        github_notification: Box<GithubNotification>,
        github_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        seed_user_data(
            &app,
            &settings,
            &github_notification,
            github_oauth_credential,
        )
        .await;

        let owned_before = rows_owned_by(&app.app, app.user.id).await;
        for table in [
            // Recorded by the audit triggers when the notification is seeded
            "audit_event.user_id",
            "authentication_token.user_id",
            "integration_connection.user_id",
            "notification.user_id",
            "slack_bridge_pending_action.user_id",
            "third_party_item.user_id",
        ] {
            assert!(owned_before[table] > 0, "{table} should have been seeded");
        }

        let email = app.user.email.as_ref().unwrap().expose().to_string();
        let response = delete_current_user_response(&app.client, &app.app, &email).await;
        assert_eq!(response.status(), 200);
        let body: SuccessResponse = response.json().await.unwrap();
        assert!(body.success);

        // Every row owned by the user is gone.
        let owned_after = rows_owned_by(&app.app, app.user.id).await;
        assert!(
            owned_after.values().all(|count| *count == 0),
            "some user data survived the account deletion: {owned_after:?}"
        );
        let oauth_credentials: i64 = sqlx::query_scalar("SELECT count(*) FROM oauth_credential")
            .fetch_one(&*app.app.repository.pool)
            .await
            .unwrap();
        assert_eq!(oauth_credentials, 0);

        // The session was cleared.
        let response = get_current_user_response(&app.client, &app.app).await;
        assert_eq!(response.status(), 401);
    }

    #[rstest]
    #[case::empty("")]
    #[case::wrong_email("someone.else@example.com")]
    #[case::keyword("DELETE")]
    #[tokio::test]
    async fn test_account_deletion_requires_the_email_confirmation(
        #[future] authenticated_app: AuthenticatedApp,
        #[case] confirmation: &str,
    ) {
        let app = authenticated_app.await;

        let response = delete_current_user_response(&app.client, &app.app, confirmation).await;
        assert_eq!(response.status(), 400);

        let user = get_current_user(&app.client, &app.app).await;
        assert_eq!(user.id, app.user.id);
    }

    #[rstest]
    #[tokio::test]
    async fn test_account_deletion_requires_authentication(#[future] tested_app: TestedApp) {
        let app = tested_app.await;

        let response =
            delete_current_user_response(&reqwest::Client::new(), &app, "john@doe.name").await;
        assert_eq!(response.status(), 401);
    }
}
