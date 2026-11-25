Feature: Receiving Notion webhook events
  Notion tells the app about comments and page changes. The app only queues the events that are signed,
  of a type it handles, well formed, and of a workspace that a user connected.

  Background:
    Given I am signed in

  @legacy:test_notion_webhook::route::test_verification_handshake_is_acknowledged
  Scenario: The verification handshake of Notion is acknowledged
    When Notion sends the verification handshake
    Then the app answers 200
    And 0 Notion events are queued

  @legacy:test_notion_webhook::route::test_unsigned_or_badly_signed_event_is_rejected
  Scenario: An event without a valid signature is refused
    When Notion sends a comment event without a signature
    Then the app answers 401
    When Notion sends a comment event with a wrong signature
    Then the app answers 401
    And 0 Notion events are queued

  Rule: An event of a connected workspace is queued

    Background:
      Given I am connected to Notion, in a workspace of its own

    @legacy:test_notion_webhook::route::test_comment_event_of_a_connected_workspace_is_queued
    Scenario: A comment event is queued
      When Notion sends a comment event of my workspace
      Then the app answers 200
      And 1 Notion event is queued

    @legacy:test_notion_webhook::route::test_page_content_event_of_a_connected_workspace_is_queued
    Scenario: A page content event is queued
      When Notion sends a page content event of my workspace
      Then the app answers 200
      And 1 Notion event is queued

    @legacy:test_notion_webhook::route::test_other_event_types_are_dropped
    Scenario: An event of another type is dropped
      When Notion sends a event of another type of my workspace
      Then the app answers 200
      And 0 Notion events are queued

    @legacy:test_notion_webhook::route::test_event_with_a_malformed_resource_id_is_dropped::case_1_comment_page_id
    Scenario: A comment event whose page id is malformed is dropped
      When Notion sends a comment event whose page id is malformed of my workspace
      Then the app answers 200
      And 0 Notion events are queued

    @legacy:test_notion_webhook::route::test_event_with_a_malformed_resource_id_is_dropped::case_2_comment_parent
    Scenario: A comment event whose parent block id is malformed is dropped
      When Notion sends a comment event whose parent block id is malformed of my workspace
      Then the app answers 200
      And 0 Notion events are queued

    @legacy:test_notion_webhook::route::test_event_with_a_malformed_resource_id_is_dropped::case_3_updated_block
    Scenario: A page content event whose updated block id is malformed is dropped
      When Notion sends a page content event whose updated block id is malformed of my workspace
      Then the app answers 200
      And 0 Notion events are queued

    @legacy:test_notion_webhook::route::test_events_of_a_disconnected_workspace_are_dropped
    Scenario: Once the connection is deleted, the events of its workspace are dropped
      When Notion sends a comment event of my workspace
      Then 1 Notion event is queued
      When I delete my Notion connection
      And Notion sends a comment event of my workspace
      Then the app answers 200
      And 1 Notion event is queued

  @legacy:test_notion_webhook::route::test_comment_event_of_an_unknown_workspace_is_dropped
  Scenario: A comment event of a workspace that nobody connected is dropped
    When Notion sends a comment event of a workspace that nobody connected
    Then the app answers 200
    And 0 Notion events are queued
