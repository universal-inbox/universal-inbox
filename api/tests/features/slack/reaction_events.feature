Feature: Slack reaction events
  A Slack reaction on a message becomes a notification or a task of the user who reacted,
  according to their Slack connection: nothing is made for a user who is not connected,
  whose sync is disabled, or who watches another reaction. The removal of the reaction
  deletes the notification or completes the task, and the same event twice changes nothing.

  Background:
    Given I am signed in

  Rule: A reaction that is not for me is ignored

    @legacy:test_slack_webhook_star_reaction::test_receive_reaction_event_for_unknown_user
    Scenario: A reaction when I am not connected to Slack
      When Slack delivers the event of the reaction "eyes" being added
      Then the event is ignored
      And I have no unread notification

    @legacy:test_slack_webhook_star_reaction::test_receive_reaction_event_for_disabled_config
    Scenario: A reaction when the sync of my Slack connection is disabled
      Given I am connected to Slack, with its sync disabled
      When Slack delivers the event of the reaction "eyes" being added
      Then the event is ignored
      And I have no unread notification

    @legacy:test_slack_webhook_star_reaction::test_receive_reaction_event_for_different_reaction
    Scenario: A reaction other than the one I sync
      Given I am connected to Slack, syncing the reaction "bookmark" as notifications
      When Slack delivers the event of the reaction "eyes" being added
      Then the event is ignored
      And I have no unread notification

    Scenario: A reaction of another Slack user is not mine
      Given I am connected to Slack, syncing its notifications, as the Slack user "U05OTHER"
      When Slack delivers the event of the reaction "eyes" being added
      Then the event is ignored
      And I have no unread notification

    Scenario: A reaction of the Slack user I am connected as is mine
      Given I am connected to Slack, syncing its notifications, as the Slack user "U05XXX"
      When Slack delivers the event of the reaction "eyes" being added
      Then the event is queued for processing

    Scenario: A reaction of a user connected to the same Slack team makes nothing for me
      Given I am connected to Slack, syncing its notifications, as the Slack user "U05OTHER"
      And Slack serves the message of the reaction events, with its author, channel, team and user groups
      And another user is signed in
      And that other user is connected to Slack, syncing its notifications, as the Slack user "U05XXX"
      When Slack delivers the event of the reaction "eyes" being added
      Then Slack has finished processing that event
      And that other user has one unread notification, which is the Slack message of that reaction
      And I have no unread notification

  Rule: A reaction makes a notification

    Background:
      Given I am connected to Slack, syncing its notifications
      And Slack serves the message of the reaction events, with its author, channel, team and user groups

    @legacy:test_slack_webhook_star_reaction::test_receive_reaction_added_event_as_notification
    Scenario: A reaction added becomes an unread notification, once
      When Slack delivers the event of the reaction "eyes" being added
      Then I get one unread notification, which is the Slack message of that reaction, with all its details
      When Slack delivers the same event again
      Then Slack has finished processing that event
      And I still get one unread notification, the one of that message

    @legacy:test_slack_webhook_star_reaction::test_receive_reaction_removed_event_as_notification
    Scenario: A reaction removed deletes the notification of the reaction added
      Given Slack delivers the event of the reaction "eyes" being added
      And I remember the unread notification of that message
      When Slack delivers the event of the reaction "eyes" being removed
      Then that notification is deleted, and none is unread anymore

  Rule: A reaction makes a task

    Background:
      Given I am connected to Slack, syncing its tasks
      And I am connected to Todoist
      And Todoist can list my projects
      And Slack serves the message of the reaction events, with its author, channel, team and user groups
      And Todoist adds the Slack message as an item of the Inbox, and then serves its item

    @legacy:test_slack_webhook_star_reaction::test_receive_reaction_added_event_as_task
    Scenario: A reaction added becomes an active task mirrored in Todoist, once
      When Slack delivers the event of the reaction "eyes" being added
      Then I get one active task, mirrored in Todoist, and no notification
      When Slack delivers the same event again
      Then Slack has finished processing that event
      And I still get one active task, the one of that message

    @legacy:test_slack_webhook_star_reaction::test_receive_reaction_removed_and_added_event_as_task
    Scenario: A reaction removed completes the task and the reaction added makes it active again
      Given Slack delivers the event of the reaction "eyes" being added
      And I get one active task, mirrored in Todoist, and no notification
      And Todoist completes the task "1123"
      When Slack delivers the event of the reaction "eyes" being removed
      Then that task is done, and no notification is deleted
      Given Todoist uncompletes the task "1123"
      When Slack delivers the event of the reaction "eyes" being added
      Then that task is active again

  Rule: Waiting for Slack to finish processing an event

    Scenario: The wait lasts as long as a slow job, and the effect of the job is visible when it ends
      Given I am connected to Slack, syncing its notifications
      And Slack serves the message of the reaction events, taking a second to answer for the message itself
      And Slack delivers the event of the reaction "eyes" being added
      When I measure how long Slack takes to finish processing that event
      Then that wait lasted as long as the slow answer of Slack
      And my notifications list, read once without waiting, has the notification of that message
