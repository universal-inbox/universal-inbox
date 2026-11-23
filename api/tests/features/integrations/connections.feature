Feature: Integration connections
  A user connects the providers they use. A connection lists in their account
  only, can be disconnected without losing the notifications it fed, and keeps
  working or fails depending on whether its access token can be refreshed.

  Background:
    Given I am signed in

  Rule: Listing and creating connections

    @legacy:test_integration_connections::list_integration_connections::test_empty_list_integration_connections
    Scenario: A user without connections sees none
      When I list my integration connections
      Then I see no integration connections

    @legacy:test_integration_connections::list_integration_connections::test_list_integration_connections
    Scenario: Connections are listed by id, and only to their owner
      Given I have created a GitHub integration connection
      And I have created a Todoist integration connection
      And another user is signed in
      When I list my integration connections
      Then I see exactly the connections I created, ordered by id
      When that other user lists their integration connections
      Then I see no integration connections

    @legacy:test_integration_connections::create_integration_connections::test_create_integration_connection
    Scenario: A user creates a connection
      When I create a GitHub integration connection
      Then the new connection is a GitHub connection of mine that was just created

  Rule: Disconnecting a connection keeps its notifications

    @legacy:test_integration_connections::disconnect_integration_connections::test_disconnect_validated_integration_connection
    Scenario: A disconnected connection sets its notifications aside without removing them
      Given I have a validated GitHub integration connection without a stored credential
      And I have a GitHub notification called "mine"
      When I disconnect my GitHub integration connection
      Then the connection is back to created, without a failure message
      And my inbox holds no notification, even counting the snoozed ones and every status
      And I can still read the notification "mine", with its status unchanged

  Rule: Looking for an access token that expired

    @legacy:test_integration_connections::find_access_token::test_find_access_token_marks_connection_failing_when_refresh_token_missing
    Scenario: A connection that cannot refresh its token is marked as failing
      Given my Google Calendar access token expired an hour ago and I have no refresh token
      When the app looks for my Google Calendar access token
      Then the lookup fails with a recoverable error
      And my Google Calendar connection is failing, because the refresh token is missing

    @legacy:test_integration_connections::find_access_token::test_find_access_token_keeps_validated_when_refresh_token_present
    Scenario: A connection that holds a refresh token stays validated
      Given my Google Calendar access token expired an hour ago and I have a refresh token
      When the app looks for my Google Calendar access token
      Then the lookup fails with a recoverable error
      And my Google Calendar connection is still validated, without a failure message
