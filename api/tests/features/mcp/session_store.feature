Feature: Storing MCP sessions
  An MCP session is kept in Redis for a limited time, so that any instance of
  the app can resume it.

  Background:
    Given the MCP session store keeps a session for 60 seconds

  Scenario: A stored session is loaded back
    When a session for the client "test-client" version "1.0.0" is stored
    And that session is loaded
    Then the loaded session is for the client "test-client" version "1.0.0"

  Scenario: A session that nobody stored is not found
    When a session that nobody stored is loaded
    Then no session is found

  Scenario: A deleted session is gone
    When a session for the client "test-client" version "1.0.0" is stored
    And that session is deleted
    And that session is loaded
    Then no session is found

  Scenario: A stored session expires after the time the store keeps it
    Given the MCP session store keeps a session for 600 seconds
    When a session for the client "test-client" version "1.0.0" is stored
    Then the stored session expires within the 600 seconds it is kept
