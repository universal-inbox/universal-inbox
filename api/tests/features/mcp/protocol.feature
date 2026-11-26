Feature: The MCP endpoint
  AI clients talk to the app through MCP, with an API token as bearer token.
  Every MCP session belongs to the user who opened it.

  Background:
    Given I am signed in
    And I have an API token

  Scenario: An MCP session announces the server and lists the tools
    When I initialize an MCP session with my API token
    Then the MCP server speaks protocol version "2025-06-18" as "universal-inbox"
    And the MCP server does not announce changes of its tool list
    When I list the tools of my MCP session
    Then the MCP server lists these tools:
      | list_notifications              |
      | bulk_act_notifications          |
      | create_task_from_notification   |
      | list_tasks                      |
      | update_task                     |
      | search_audit_log                |

  Scenario: Nobody else can read, use or close the MCP session of a user
    Given an MCP session is open with my API token
    And another user is signed in
    And the other user has an API token of their own
    When the other user replays the event stream of my MCP session
    Then the MCP request is answered with status 404
    When the other user lists the tools of my MCP session
    Then the MCP request is answered with status 404
    When the other user closes my MCP session
    Then the MCP request is answered with status 404
    When the other user lists the tools of an MCP session that nobody opened
    Then the MCP request is answered with status 404
    When I list the tools of my MCP session
    Then the MCP request is answered with status 200
    And the MCP server lists its tools

  Scenario: An MCP session cannot be opened without a token
    When I try to initialize an MCP session without any token
    Then the MCP request is answered with status 401

  Scenario: The protocol version header is optional, but must be a valid one when given
    When I list the tools of a new MCP session, announcing no protocol version
    Then the MCP request is answered with status 200
    And the MCP server lists its tools
    When I list the tools of a new MCP session, announcing the protocol version "invalid-version"
    Then the MCP request is answered with status 400
    When I list the tools of a new MCP session, announcing the protocol version "2025-06-18"
    Then the MCP request is answered with status 200
    And the MCP server lists its tools

  Scenario: An unknown tool and invalid arguments are protocol errors
    When I call the MCP tool "unknown_tool" in a new session, with:
      """
      {}
      """
    Then the MCP call is rejected as invalid parameters
    When I call the MCP tool "act_on_notification" in a new session, with:
      """
      { "notification_id": "6b7ba4d5-71f4-4d1e-8c58-3cfb1ac1b8ea", "action": "snooze_until" }
      """
    Then the MCP call is rejected as invalid parameters
    When I update a task in a new MCP session with a title one character over the limit
    Then the MCP call is rejected as invalid parameters

  Scenario: A malformed bulk request is rejected and applies nothing
    Given I am connected to GitHub
    And I have a GitHub notification
    When I ask in a new MCP session to bulk act on notifications with a request with both modes
    Then the MCP call is rejected as invalid arguments
    And my GitHub notification is still unread and not snoozed
    When I ask in a new MCP session to bulk act on notifications with a request with no mode
    Then the MCP call is rejected as invalid arguments
    And my GitHub notification is still unread and not snoozed
    When I ask in a new MCP session to bulk act on notifications with an unfiltered sweep without confirmation
    Then the MCP call is rejected as invalid arguments
    And my GitHub notification is still unread and not snoozed
    When I ask in a new MCP session to bulk act on notifications with an unfiltered sweep that refuses confirmation
    Then the MCP call is rejected as invalid arguments
    And my GitHub notification is still unread and not snoozed
    When I ask in a new MCP session to bulk act on notifications with an empty list
    Then the MCP call is rejected as invalid arguments
    And my GitHub notification is still unread and not snoozed
    When I ask in a new MCP session to bulk act on notifications with a list of 101 entries
    Then the MCP call is rejected as invalid arguments
    And my GitHub notification is still unread and not snoozed
    When I ask in a new MCP session to bulk act on notifications with a list naming the notification twice
    Then the MCP call is rejected as invalid arguments
    And my GitHub notification is still unread and not snoozed
    When I ask in a new MCP session to bulk act on notifications with a snooze without a date
    Then the MCP call is rejected as invalid arguments
    And my GitHub notification is still unread and not snoozed
    When I ask in a new MCP session to bulk act on notifications with an unfiltered sweep that confirms
    Then that call acted on 1 notification

  Scenario: A session opened on another instance of the app is restored from Redis
    Given my MCP session was opened on another instance of the app, which shared it through Redis
    When I list my notifications in that session
    Then the MCP request is answered with status 200
    And the answer is not an error

  Scenario: A session that nobody opened is not found
    When I list the tools of an MCP session that nobody opened
    Then the MCP request is answered with status 404

  Scenario: Opening an MCP session stores it in Redis
    When I initialize an MCP session with my API token
    Then my MCP session is stored with the client name "test-client"

  Scenario: The host of the front end is accepted
    When I try to initialize an MCP session addressed to the host of the front end
    Then the MCP request is answered with status 200

  Scenario: An unknown host is refused
    When I try to initialize an MCP session addressed to the host "evil.example.com"
    Then the MCP request is answered with status 403
