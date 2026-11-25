Feature: Authorizing an OAuth2 client
  A signed-in user lets an MCP client act for them. The first time, the user must
  consent. The client then trades the authorization code for tokens, proving it
  started the request with a PKCE verifier.

  Background:
    Given I am signed in
    And I have an API token
    And an OAuth2 client is registered for a loopback callback

  Rule: The user consents before the client gets a code

    @legacy:test_mcp::oauth2::authorize_redirects_to_consent_when_not_yet_granted
    Scenario: A client that was not consented to yet is sent to the consent screen, without a code
      Given my PKCE verifier is "verifier-no-consent"
      When I ask to authorize that client
      Then the app answers 302
      And the response header "location" contains "/oauth2/consent?request_id="
      And the response header "location" does not contain "code="

    @legacy:test_mcp::oauth2::authorize_issues_code_when_consent_already_granted
    Scenario: A client that was consented to gets its code straight away
      Given my PKCE verifier is "verifier-with-consent"
      And I already consented to that client for the scope "read write"
      When I ask to authorize that client
      Then the redirect of the authorization goes back to the client, with a code and the state it sent

    @legacy:test_mcp::oauth2::consent_post_allow_creates_code_and_consent_row
    Scenario: Allowing a client gives it a code that can be exchanged, and remembers the consent
      Given my PKCE verifier is "consent-allow-verifier"
      When I ask to authorize that client
      And I open the consent screen of that request
      And I allow that request
      Then the decision redirects with an authorization code
      When I exchange that code with my PKCE verifier
      Then the token response is a bearer token that lasts 3600 seconds
      Given my PKCE verifier is "second-verifier"
      When I ask to authorize that client
      Then the redirect of the authorization goes back to the client, with a code and the state it sent

    @legacy:test_mcp::oauth2::consent_post_deny_returns_access_denied
    Scenario: Denying a client gives it an error, not a code
      Given my PKCE verifier is "verifier-deny"
      When I ask to authorize that client
      And I open the consent screen of that request
      And I deny that request
      Then the decision redirects with the error "access_denied" and no authorization code

    @legacy:test_mcp::oauth2::consent_post_rejects_invalid_csrf
    Scenario: A consent answer without the right CSRF token is refused
      Given my PKCE verifier is "verifier-bad-csrf"
      When I ask to authorize that client
      And I allow that request, with the CSRF token "not-the-real-token"
      Then the app answers 403

  Rule: The scope of the token is the one the user consented to

    Background:
      Given I already consented to that client for the scope "read write"
      And my PKCE verifier is "scope-test-verifier-0123456789-abcdefghijklmnop"

    @legacy:test_mcp::oauth2::authorize_normalizes_requested_scope::case_1_absent
    Scenario: Without a requested scope, the token can read and write
      When I ask to authorize that client for the MCP resource
      And I exchange that code with my PKCE verifier
      Then the token response has the scope "read write"

    @legacy:test_mcp::oauth2::authorize_normalizes_requested_scope::case_2_unknown_tokens_dropped
    Scenario: Scopes that do not exist are dropped
      When I ask to authorize that client for the MCP resource with the scope "read admin offline_access"
      And I exchange that code with my PKCE verifier
      Then the token response has the scope "read"

    @legacy:test_mcp::oauth2::authorize_normalizes_requested_scope::case_3_canonical_order
    Scenario: The scopes come in the canonical order
      When I ask to authorize that client for the MCP resource with the scope "write read"
      And I exchange that code with my PKCE verifier
      Then the token response has the scope "read write"

    @legacy:test_mcp::oauth2::authorize_rejects_only_unsupported_scopes
    Scenario: A request for scopes that all do not exist is refused
      When I ask to authorize that client with the scope "admin"
      Then the app answers 400

  Rule: The code is only worth something to the client that holds the PKCE verifier

    @legacy:test_mcp::oauth2::oauth2_rejects_invalid_pkce_verifier
    Scenario: A code exchanged with another verifier is refused
      Given my PKCE verifier is "correct-verifier-value"
      And I already consented to that client for the scope "read write"
      When I ask to authorize that client for the MCP resource with the scope "read write"
      And I exchange that code with the PKCE verifier "wrong-verifier-value"
      Then the app answers 400
