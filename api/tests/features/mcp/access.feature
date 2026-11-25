Feature: Reaching the MCP endpoint
  MCP clients discover how to authenticate, then call the endpoint with a bearer token.
  A session cookie, a foreign origin and a missing session are refused.

  @legacy:test_mcp::oauth2::well_known_metadata_endpoints
  Scenario: The app publishes where and how to authenticate
    When I request "/.well-known/oauth-protected-resource"
    Then the app answers 200
    And the response field "resource" ends with "/mcp"
    And the response field "authorization_servers" is not empty
    And the response field "bearer_methods_supported" is:
      """
      ["header"]
      """
    And the response field "scopes_supported" is:
      """
      ["read", "write"]
      """
    When I request "/.well-known/oauth-protected-resource/api/mcp"
    Then the app answers 200
    And the response field "resource" ends with "/mcp"
    When I request "/.well-known/oauth-authorization-server"
    Then the app answers 200
    And the response field "authorization_endpoint" is not empty
    And the response field "token_endpoint" is not empty
    And the response field "registration_endpoint" is not empty
    And the response field "response_types_supported" is:
      """
      ["code"]
      """
    And the response field "grant_types_supported" is:
      """
      ["authorization_code", "refresh_token"]
      """
    And the response field "code_challenge_methods_supported" is:
      """
      ["S256"]
      """
    And the response field "token_endpoint_auth_methods_supported" is:
      """
      ["none"]
      """
    And the response field "resource_indicators_supported" is:
      """
      true
      """
    And the response field "client_id_metadata_document_supported" is:
      """
      true
      """

  @legacy:test_mcp::oauth2::mcp_returns_www_authenticate_without_token
  Scenario: A request without a token is told where to authenticate
    When I try to initialize an MCP session without any token
    Then the MCP request is answered with status 401
    And the response header "www-authenticate" starts with "Bearer resource_metadata="
    And the response header "www-authenticate" contains "/.well-known/oauth-protected-resource"

  Rule: The endpoint only takes bearer tokens, from its own origin

    Background:
      Given I am signed in
      And I have an API token

    @legacy:test_mcp::oauth2::mcp_get_without_session_returns_400
    Scenario: The event stream cannot be opened without a session
      When I open the event stream of MCP without a session
      Then the MCP request is answered with status 400

    @legacy:test_mcp::oauth2::mcp_rejects_session_cookie_auth
    Scenario: A session cookie is not accepted, so that a web page cannot act for the user
      When I try to initialize an MCP session with my session cookie only
      Then the MCP request is answered with status 401
      And the response header "www-authenticate" starts with "Bearer resource_metadata="

    @legacy:test_mcp::oauth2::mcp_rejects_invalid_origin
    Scenario: A request from a foreign origin is refused
      When I try to initialize an MCP session from the origin "https://evil.example.com"
      Then the MCP request is answered with status 403

    @legacy:test_mcp::rate_limits::sync_tools_share_the_per_user_sync_budget
    Scenario: The sync tools share the sync budget of the user
      When I use the whole sync budget of a minute through MCP, alternating notifications and tasks
      And I ask through MCP to sync my tasks once more
      Then that sync is refused for too many requests
