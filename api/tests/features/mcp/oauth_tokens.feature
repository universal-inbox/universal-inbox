Feature: OAuth2 tokens for MCP
  An authorized MCP client holds an access token, and a refresh token that it
  trades for a new pair. The access token only works on MCP, with the scope that
  the user consented to.

  Background:
    Given I am signed in
    And I have an API token

  Rule: The tokens give access to MCP, and to nothing else

    Scenario: A client gets its tokens, uses MCP, refreshes its token and uses MCP again
      Given an OAuth2 client is registered for a loopback callback
      And my PKCE verifier is "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
      And I already consented to that client for the scope "read write"
      When I ask to authorize that client for the MCP resource with the scope "read write"
      And I exchange that code with my PKCE verifier
      Then the token response is a bearer token that lasts 3600 seconds
      And the token response has the scope "read write"
      When I initialize an MCP session with that OAuth2 token
      Then the MCP session gets an id
      And the MCP server speaks protocol version "2025-06-18" as "universal-inbox"
      When I list the tools of that MCP session with that OAuth2 token
      Then the MCP server lists at least 11 tools
      When I refresh that OAuth2 token
      Then the app gave me a new access token
      When I initialize an MCP session with that OAuth2 token
      Then the MCP session gets an id

    Scenario: A token issued for another resource is refused by MCP
      Given an OAuth2 client is registered for a loopback callback
      And my PKCE verifier is "audience-test-verifier"
      And I already consented to that client for the scope "read write"
      When I ask to authorize that client for the resource "https://wrong-resource.example.com"
      And I exchange that code with my PKCE verifier
      And I try to initialize an MCP session with that OAuth2 token
      Then the MCP request is answered with status 403

    Scenario: A token for MCP is refused on the rest of the API, while the session of the user still works
      Given I hold an OAuth2 access token for MCP with the scope "read write"
      When I call the app API at "users/me" with that OAuth2 token
      Then the app answers 401
      When I request "/api/users/me"
      Then the app answers 200

  Rule: The scope of the token limits what MCP lets it do

    Scenario: A token that can only read is refused every tool that changes something
      Given I hold an OAuth2 access token for MCP with the scope "read"
      Then the token response has the scope "read"
      When I call the MCP tool "list_notifications" with that OAuth2 token, with:
        """
        { "trigger_sync": false }
        """
      Then the answer is not an error
      When I call these MCP tools with that OAuth2 token:
        | tool                   | arguments                                                                              |
        | bulk_act_notifications | {"mode": "filter", "action": "delete"}                                                 |
        | act_on_notification    | {"notification_id": "6b7ba4d5-71f4-4d1e-8c58-3cfb1ac1b8ea", "action": "delete"}         |
        | update_task            | {"task_id": "6b7ba4d5-71f4-4d1e-8c58-3cfb1ac1b8ea", "patch": {"status": "Done"}}        |
        | sync_notifications     | {}                                                                                     |
        | sync_tasks             | {}                                                                                     |
      Then each of those calls is refused for lack of scope

    Scenario: A token that can read and write may use the tools that change something
      Given I hold an OAuth2 access token for MCP with the scope "read write"
      When I call the MCP tool "bulk_act_notifications" with that OAuth2 token, with:
        """
        { "mode": "filter", "statuses": ["Read"], "action": "delete" }
        """
      Then the answer is not an error

  Rule: A refresh token is used once, and reusing it ends the whole family

    Scenario: Two refreshes at the same time with one refresh token: one wins, and the family is revoked
      Given I hold an OAuth2 access token for MCP with the scope "read write"
      When I refresh that OAuth2 token twice at the same time
      Then exactly one of the two refreshes succeeded and the other was refused with 400
      When I refresh with the latest refresh token that the app gave me
      Then the app answers 400

    Scenario: Replaying a used refresh token is refused and revokes the refresh token that replaced it
      Given I hold an OAuth2 access token for MCP with the scope "read write"
      When I refresh that OAuth2 token
      Then the app gave me a new access token
      When I replay the refresh token of the first exchange
      Then the app answers 400
      When I refresh with the latest refresh token that the app gave me
      Then the app answers 400

  Rule: The changes made through MCP are recorded in the audit log

    Scenario: A change made by an MCP client is recorded with that client, and only its user can find it
      Given I am connected to GitHub
      And I have a GitHub notification
      And I hold an OAuth2 access token for MCP with the scope "read write"
      When I snooze my GitHub notification until "2064-01-01T09:00:00Z" through MCP, with that OAuth2 token
      Then the answer is not an error
      Given I hold an OAuth2 access token for MCP with the scope "read"
      When I search the audit log through MCP, with that OAuth2 token, for the changes that MCP made to my GitHub notification
      Then the audit log has 1 change
      And that change was made by the MCP client named "test-mcp-client"
      And that change moved the snooze date to "2064-01-01T09:00:00"
      Given another user is signed in
      And the other user has an API token of their own
      When the other user searches the audit log through MCP for the changes to my GitHub notification
      Then the audit log has 0 changes
