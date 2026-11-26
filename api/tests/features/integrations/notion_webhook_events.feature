Feature: Receiving Notion webhook events
  Notion tells the app about comments and page changes. The app only queues the events that are signed,
  of a type it handles, well formed, and of a workspace that a user connected.

  Background:
    Given I am signed in

  Scenario: The verification handshake of Notion is acknowledged
    When Notion sends the verification handshake
    Then the app answers 200
    And 0 Notion events are queued

  Rule: An event is queued when it is signed, handled, well formed and of a connected workspace

    Background:
      Given I am connected to Notion, in a workspace of its own

    Scenario: A comment event is queued
      When Notion sends a comment event of my workspace
      Then the app answers 200
      And 1 Notion event is queued

    Scenario: A page content event is queued
      When Notion sends a page content event of my workspace
      Then the app answers 200
      And 1 Notion event is queued

    Scenario: An event without a valid signature is refused and not queued, while the same event signed is queued
      When Notion sends a comment event of my workspace, without a signature
      Then the app answers 401
      When Notion sends a comment event of my workspace, with a wrong signature
      Then the app answers 401
      And 0 Notion events are queued
      When Notion sends a comment event of my workspace
      Then the app answers 200
      And 1 Notion event is queued

    Scenario: A comment event of another workspace is dropped, while the one of my workspace is queued
      When Notion sends a comment event of another workspace
      Then the app answers 200
      And 0 Notion events are queued
      When Notion sends a comment event of my workspace
      Then the app answers 200
      And 1 Notion event is queued

    Scenario: An event of another type is dropped
      When Notion sends an event of another type of my workspace
      Then the app answers 200
      And 0 Notion events are queued

    Scenario: A comment event whose page id is malformed is dropped
      When Notion sends a comment event whose page id is malformed of my workspace
      Then the app answers 200
      And 0 Notion events are queued

    Scenario: A comment event whose parent block id is malformed is dropped
      When Notion sends a comment event whose parent block id is malformed of my workspace
      Then the app answers 200
      And 0 Notion events are queued

    Scenario: A page content event whose updated block id is malformed is dropped
      When Notion sends a page content event whose updated block id is malformed of my workspace
      Then the app answers 200
      And 0 Notion events are queued

    Scenario: Once the connection is deleted, the events of its workspace are dropped
      When Notion sends a comment event of my workspace
      Then 1 Notion event is queued
      When I delete my Notion connection
      And Notion sends a comment event of my workspace
      Then the app answers 200
      And 1 Notion event is queued
