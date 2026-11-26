Feature: Integration connections
  A user connects the providers they use. A connection lists in their account
  only, can be disconnected without losing the notifications it fed, and keeps
  working or fails depending on whether its access token can be refreshed.

  Background:
    Given I am signed in

  Rule: Listing and creating connections

    Scenario: A user without connections sees none
      When I list my integration connections
      Then I see no integration connections

    Scenario: Connections are listed by id, and only to their owner
      Given I have created a GitHub integration connection
      And I have created a Todoist integration connection
      And another user is signed in
      When I list my integration connections
      Then I see exactly the connections I created, ordered by id
      When that other user lists their integration connections
      Then I see no integration connections

    Scenario: A user creates a connection
      When I create a GitHub integration connection
      Then the new connection is a GitHub connection of mine that was just created

  Rule: Disconnecting a connection keeps its notifications

    Scenario: A disconnected connection sets its notifications aside without removing them
      Given I have a validated GitHub integration connection without a stored credential
      And I have a GitHub notification called "mine"
      When I disconnect my GitHub integration connection
      Then the connection is back to created, without a failure message
      And my inbox holds no notification, even counting the snoozed ones and every status
      And I can still read the notification "mine", with its status unchanged

  Rule: Looking for an access token that expired

    Scenario: A connection that cannot refresh its token is marked as failing
      Given my Google Calendar access token expired an hour ago and I have no refresh token
      When the app looks for my Google Calendar access token
      Then the lookup fails with a recoverable error
      And my Google Calendar connection is failing, because the refresh token is missing

    Scenario: A connection that holds a refresh token stays validated
      Given my Google Calendar access token expired an hour ago and I have a refresh token
      When the app looks for my Google Calendar access token
      Then the lookup fails with a recoverable error
      And my Google Calendar connection is still validated, without a failure message

  Rule: Saving the configuration of a connection

    Scenario: Saving a configuration changes that connection only and leaves the inbox as it was
      Given I have a validated Google Mail connection for the address "test@example.com" that synchronises the label "Label 1"
      And I have a validated GitHub integration connection without a stored credential
      And I have a notification called "existing" from a thread of my Google Mail connection
      When I change the synchronised label of my Google Mail connection to "Label 2"
      Then the saved configuration synchronises the label "Label 2"
      And my Google Mail connection synchronises the label "Label 2" and still knows my address "test@example.com"
      And my other GitHub connection is unchanged
      And my inbox holds exactly the notification "existing", as it was

    Scenario: Another user cannot tell a connection of mine from one that does not exist
      Given I have a validated Google Mail connection that synchronises the label "Label 1"
      And another user is signed in
      When that other user switches off the notifications of my Google Mail connection and of a connection that does not exist
      Then both answers are 404 Not Found with the same message, which only repeats the id that was asked for
      And my Google Mail connection still synchronises the label "Label 1", as before

    Scenario: A connection paused by my plan cannot be switched back on through its reaction sync
      Given I have a validated Slack connection whose every sync is off, paused by my plan
      When I turn on the reaction sync of my Slack connection, leaving the message sync off
      Then the app answers 402
      And my Slack connection still holds its paused configuration, and stays paused by my plan
      When I change the reaction of my Slack connection to "bookmark", leaving every sync off
      Then the app answers 200

  Rule: Storing the credential of a connection

    Scenario: The tokens of a provider response are never kept in the clear
      Given I have a validated Slack connection that synchronises my notifications
      When a Slack token response holding cleartext tokens is stored for that connection
      Then the stored token response keeps the Slack user id and none of the tokens
