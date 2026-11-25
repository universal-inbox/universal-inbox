Feature: The OAuth callback
  The provider sends the user back with a state and a code. The callback refuses a
  state issued to somebody else before the code is exchanged, never echoes what
  the provider or an internal error said, and pins the provider account on the
  connection.

  Background:
    Given I am signed in

  Rule: A callback that cannot be trusted is refused

    @legacy:test_oauth_callback::test_oauth_callback_rejects_state_issued_to_another_user
    Scenario: A state issued to me is refused for another signed in user
      Given I started the OAuth flow of a new TickTick connection
      And another user is signed in
      When the provider calls back with that state and a code, for another signed in user
      Then I am redirected to my settings with "oauth_error=invalid-state"
      And TickTick was not called by that callback
      And that redirect does not contain "Failed to retrieve"
      And that redirect does not contain "redis"
      And that redirect does not contain "invalid or expired"
      And my connection is still not validated, and not pinned to any account
      And no credential is stored for my connection

    @legacy:test_oauth_callback::test_oauth_callback_requires_a_session
    Scenario: A callback without session is refused and consumes nothing
      Given I started the OAuth flow of a new TickTick connection
      When the provider calls back with that state and a code, without any session
      Then I am redirected to my settings with "oauth_error=invalid-state"
      And TickTick was not called by that callback
      And the state of that flow is still stored

    @legacy:test_oauth_callback::test_oauth_callback_redacts_internal_error_chain
    Scenario: A state nobody issued is reported with a public code only
      When the provider calls back with a state nobody issued, for me
      Then that redirect does not contain "Failed to retrieve"
      And that redirect does not contain "redis"
      And that redirect does not contain "invalid or expired"
      And I am redirected to my settings with "oauth_error=invalid-state"

    @legacy:test_oauth_callback::test_oauth_callback_redacts_provider_error_parameter
    Scenario: The error of the provider is not echoed back
      When the provider calls back with the error "access_denied: user clicked deny on consent screen"
      Then I am redirected to my settings with "oauth_error=provider-error"
      And that redirect does not contain "access_denied"
      And that redirect does not contain "consent screen"

    @legacy:test_oauth_callback::test_oauth_callback_missing_state_returns_invalid_state
    Scenario: A callback without state is reported as an invalid state
      When the provider calls back with a code and no state
      Then I am redirected to my settings with "oauth_error=invalid-state"

  Rule: The callback pins the Google account of the connection

    Scenario Outline: A new <provider> connection is pinned to the account Google reports
      Given I started the OAuth flow of a new <provider> connection
      And the provider exchanges the code for an access token of <provider>
      And <provider> tells that the account of the access token is "Jane.Doe@example.com"
      When the provider calls back with that state and a code, for the owner of the connection
      Then I am redirected to my settings with "oauth_success=true"
      And my connection is validated, and pinned to the Google account of "jane.doe@example.com"
      And the credential of my connection is stored

      @legacy:test_oauth_callback::google_provider_user_id::test_oauth_callback_pins_google_account::case_1_google_mail
      Examples:
        | provider    |
        | Google Mail |

      @legacy:test_oauth_callback::google_provider_user_id::test_oauth_callback_pins_google_account::case_2_google_calendar
      Examples:
        | provider        |
        | Google Calendar |

      @legacy:test_oauth_callback::google_provider_user_id::test_oauth_callback_pins_google_account::case_3_google_drive
      Examples:
        | provider     |
        | Google Drive |

    @legacy:test_oauth_callback::google_provider_user_id::test_google_calendar_authorize_url_requests_primary_calendar_read_scope
    Scenario: The Google Calendar authorization asks to read the primary calendar
      Given I started the OAuth flow of a new Google Calendar connection
      Then the Google Calendar authorization asks for the scope to read the primary calendar, and the one to write events

    @legacy:test_oauth_callback::google_provider_user_id::test_oauth_callback_accepts_reconnect_with_pinned_google_account
    Scenario: A connection reconnected with the Google account it is pinned to is validated again
      Given I started the OAuth flow of a new Google Mail connection, pinned to the Google account of "jane.doe@example.com"
      And the provider exchanges the code for an access token of Google Mail
      And Google Mail tells that the account of the access token is "jane.doe@example.com"
      When the provider calls back with that state and a code, for the owner of the connection
      Then I am redirected to my settings with "oauth_success=true"
      And my connection is validated, and pinned to the Google account of "jane.doe@example.com"
      And the credential of my connection is stored

    @legacy:test_oauth_callback::google_provider_user_id::test_oauth_callback_repins_reconnect_with_another_google_account
    Scenario: A connection reconnected with another Google account is pinned to it, and its cached address follows
      Given I started the OAuth flow of a new Google Mail connection, pinned to the Google account of "jane.doe@example.com" and caching that address
      And the provider exchanges the code for an access token of Google Mail
      And Google Mail tells that the account of the access token is "John.Roe@example.com", once for the callback and once for the next sync
      When the provider calls back with that state and a code, for the owner of the connection
      Then I am redirected to my settings with "oauth_success=true"
      And my connection is validated, and pinned to the Google account of "john.roe@example.com"
      And the credential of my connection is stored
      When I sync my Google Mail notifications, which has no thread
      Then my Google Mail connection caches the address "John.Roe@example.com"

    @legacy:test_oauth_callback::google_provider_user_id::test_oauth_callback_fails_when_google_account_cannot_be_read
    Scenario: A callback that cannot read the Google account fails closed
      Given I started the OAuth flow of a new Google Drive connection
      And the provider exchanges the code for an access token of Google Drive
      And Google Drive fails when asked who the access token belongs to
      When the provider calls back with that state and a code, for the owner of the connection
      Then I am redirected to my settings with "oauth_error=internal-error"
      And my connection is still not validated, and not pinned to any account
      And no credential is stored for my connection
      And that redirect does not contain "Failed to retrieve"
      And that redirect does not contain "redis"
      And that redirect does not contain "invalid or expired"

  Rule: The callback pins the Notion user of the connection

    @legacy:test_oauth_callback::notion_provider::test_notion_authorize_url_asks_for_a_user_owner
    Scenario: The Notion authorization asks for a user owner
      Given I started the OAuth flow of a new Notion connection
      Then the Notion authorization asks for a user owner, with the client id and no scope

    @legacy:test_oauth_callback::notion_provider::test_oauth_callback_pins_notion_owner_and_stores_context
    Scenario: The callback pins the Notion user and stores the workspace
      Given I started the OAuth flow of a new Notion connection
      And Notion exchanges the code for a token of the user "notion-user-id" in the workspace "notion-workspace-id"
      When the provider calls back with that state and a code, for the owner of the connection
      Then I am redirected to my settings with "oauth_success=true"
      And my Notion connection is validated, pinned to the Notion user, and keeps the workspace and bot
      And the credential of my connection is stored

    @legacy:test_oauth_callback::notion_provider::test_notion_events_are_queued_once_the_workspace_is_connected
    Scenario: The events of a workspace are queued once the workspace is connected
      When Notion sends an event of that workspace
      Then 0 Notion events are queued
      Given I started the OAuth flow of a new Notion connection
      And Notion exchanges the code for a token of the user "notion-user-id" in the workspace "notion-workspace-id"
      When the provider calls back with that state and a code, for the owner of the connection
      And Notion sends an event of that workspace
      Then 1 Notion event is queued

  Rule: An authorization cannot be started or completed for a connection of somebody else

    Scenario: Another user cannot start the OAuth flow of my connection
      Given I have a new TickTick connection, not authorized yet
      And another user is signed in
      When the other user asks for the authorization URL of my connection
      Then that request is refused as forbidden
      And no OAuth state is stored for my connection

    Scenario: A state of mine naming a connection of another user is refused before the code is exchanged
      Given another user is signed in
      And the other user has a new Google Mail connection, not authorized yet
      And a state of mine naming that connection of the other user is stored
      And Google Mail would accept any code and say that the account is "Jane.Doe@example.com", if it were asked
      When the provider calls back with that state and a code, for the owner of the connection
      Then I am redirected to my settings with "oauth_error=invalid-state"
      And the connection of the other user is still not validated, and not pinned
      And no credential is stored for the connection of the other user
      And Google Mail was not called by that callback

    Scenario: A second callback with the same state is refused, and the credential is left untouched
      Given I started the OAuth flow of a new Google Mail connection
      And the provider exchanges the code for an access token of Google Mail
      And Google Mail tells that the account of the access token is "Jane.Doe@example.com"
      When the provider calls back with that state and a code, for the owner of the connection
      And the provider calls back with that state and a code, for the owner of the connection
      Then I am redirected to my settings with "oauth_error=invalid-state"
      And the credential of my connection is stored

    Scenario: An OAuth2 access token for MCP cannot complete a callback
      Given I started the OAuth flow of a new Google Mail connection
      And Google Mail would accept any code and say that the account is "Jane.Doe@example.com", if it were asked
      And I hold an OAuth2 access token for MCP with the scope "read write"
      When the provider calls back with that state and a code, bearing my OAuth2 access token for MCP
      Then I am redirected to my settings with "oauth_error=invalid-state"
      And Google Mail was not called by that callback
      And the state of that flow is still stored
      And no credential is stored for my connection
