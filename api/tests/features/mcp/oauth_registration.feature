Feature: Registering OAuth2 clients
  MCP clients register themselves, so they can ask a user to authorize them.
  An unknown visitor can only register a client that sends the authorization code
  back to the visitor's own machine or to a known MCP host.

  Rule: A redirect URI must be safe

    Background:
      Given I am signed in

    @legacy:test_mcp::oauth2::register_rejects_javascript_scheme
    Scenario: A redirect URI that runs a script is refused
      When I register an OAuth2 client with the redirect URI "javascript:alert(1)"
      Then the app answers 400

    @legacy:test_mcp::oauth2::register_rejects_non_loopback_http
    Scenario: A redirect URI over plain HTTP to another host is refused
      When I register an OAuth2 client with the redirect URI "http://evil.example/cb"
      Then the app answers 400

    @legacy:test_mcp::oauth2::register_rejects_http_against_allowlisted_origin
    Scenario: A redirect URI over plain HTTP to a known MCP host is still refused
      When a visitor registers an OAuth2 client with the redirect URI "http://claude.ai/cb"
      Then the app answers 400

  Rule: A visitor who is not signed in can only register a client that stays on their own machine or on a known MCP host

    Background:
      Given I am signed in

    @legacy:test_mcp::oauth2::register_requires_authentication_for_remote_redirect_uri
    Scenario: A remote redirect URI needs a signed-in user, a loopback one does not
      When a visitor registers an OAuth2 client with the redirect URI "https://example.com/cb"
      Then the app answers 401
      When a visitor registers an OAuth2 client with the redirect URI "http://localhost:1234/cb"
      Then the app answers 201
      When a visitor registers an OAuth2 client with the redirect URI "http://127.0.0.1:1234/cb"
      Then the app answers 201
      When a visitor registers an OAuth2 client with the redirect URIs "http://localhost:1234/cb" and "https://example.com/cb"
      Then the app answers 401
      When I register an OAuth2 client with the redirect URI "https://example.com/cb"
      Then the app answers 201

    @legacy:test_mcp::oauth2::register_allows_authenticated_loopback
    Scenario: A signed-in user can register a loopback redirect URI
      When I register an OAuth2 client with the redirect URI "http://localhost:1234/cb"
      Then the app answers 201
      When I register an OAuth2 client with the redirect URI "http://127.0.0.1:1234/cb"
      Then the app answers 201

    @legacy:test_mcp::oauth2::register_accepts_origin_in_mcp_allowlist
    Scenario: A visitor can register a redirect URI on a known MCP host
      When a visitor registers an OAuth2 client with the redirect URI "https://claude.ai/api/mcp/auth_callback"
      Then the app answers 201

    @legacy:test_mcp::oauth2::register_rejects_unrelated_https_host
    Scenario: A visitor cannot register a redirect URI on an unknown host
      When a visitor registers an OAuth2 client with the redirect URI "https://evil.example/cb"
      Then the app answers 401

  Rule: The registration limit is kept per client address

    Background:
      Given I am signed in

    @legacy:test_mcp::oauth2::register_rate_limit_keys_on_forwarded_ip
    Scenario: One address using up its registrations does not throttle another
      When I keep registering OAuth2 clients with the redirect URI "https://example.com/cb", from the address "1.2.3.4", up to 40 times, until I am refused
      Then I was refused for too many requests
      When I register an OAuth2 client with the redirect URI "https://example.com/cb", from the address "5.6.7.8"
      Then the app does not answer that it is refused for too many requests

    @legacy:test_mcp::oauth2::register_rejects_unspecified_forwarded_ip
    Scenario: A client whose address cannot be told is refused, not pooled with others
      When I register an OAuth2 client with the redirect URI "https://example.com/cb", from the address "0.0.0.0"
      Then the app answers 400

  @legacy:test_mcp::oauth2::oauth2_authorize_redirects_unauthenticated_to_login
  Scenario: A visitor who asks to authorize a client is sent to log in
    Given an OAuth2 client is registered for a loopback callback
    When a visitor asks to authorize that client
    Then the app answers 302
    And the response header "location" contains "login"
    And the response header "location" contains "redirect="
