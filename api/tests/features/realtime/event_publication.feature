Feature: Realtime events published by the API
  A change of the data of a user is published once, for that user, on the realtime events
  channel, when it is committed: a failed or rolled back change publishes nothing, a sync
  publishes one batched event, and the actions of the Slack bridge extension are announced
  only when the API creates them.

  Background:
    Given I am signed in
    And I am connected to GitHub

  Rule: A change of a notification

    @legacy:test_realtime_events::test_patch_notification_publishes_a_realtime_event
    Scenario: Snoozing a notification publishes one event about it
      Given I have a GitHub notification called "mine"
      And I listen to the realtime events of my user
      When I snooze "mine" until "2050-01-01T01:02:03Z"
      Then I received exactly one notifications.changed event, about "mine" only

    @legacy:test_realtime_events::test_patch_unknown_notification_publishes_nothing
    Scenario: A change of a notification that does not exist publishes nothing
      Given I have a GitHub notification called "mine"
      And I listen to the realtime events of my user
      When I mark a notification that does not exist as deleted
      Then the app answers 404
      When I snooze "mine" until "2050-01-01T01:02:03Z"
      Then I received exactly one notifications.changed event, about "mine" only

    @legacy:test_realtime_events::test_rolled_back_patch_notification_publishes_nothing
    Scenario: A change that is rolled back publishes nothing
      Given I have a GitHub notification called "mine"
      And GitHub refuses to delete the thread
      And I listen to the realtime events of my user
      When I try to delete "mine"
      Then the app answers 500
      When I snooze "mine" until "2050-01-01T01:02:03Z"
      Then I received exactly one notifications.changed event, about "mine" only

  Rule: A sync

    @legacy:test_realtime_events::test_sync_notifications_publishes_one_batched_realtime_event
    Scenario: A sync publishes one event for all its notifications, and two for its connection
      Given GitHub reports the notifications 123 and 456
      And GitHub knows pull request 123
      And GitHub serves what it reported
      And I listen to the realtime events of my user
      When I sync my GitHub notifications
      Then I received one notifications.changed event, about all the synced notifications
      And I received two integration_connection.changed events, about my GitHub connection only

  Rule: The actions of the Slack bridge extension

    Background:
      Given I am connected to Slack, syncing its notifications, with the extension bridge enabled
      And I have the Slack notification "thread"

    @legacy:test_realtime_events::slack_bridge::test_delete_slack_notification_publishes_a_pending_actions_event
    Scenario: Deleting a Slack notification creates an action and announces it
      Given I listen to the realtime events of my user
      When I delete the Slack notification "thread"
      Then I received exactly one slack_bridge_pending_actions.changed event, about the action it created

    @legacy:test_realtime_events::slack_bridge::test_complete_or_fail_pending_action_publishes_no_pending_actions_event
    Scenario: Completing or failing an action announces nothing
      Given I have two Pending Slack bridge actions that no event announced
      And I listen to the realtime events of my user
      When the Slack extension reports the first of them as completed and the second as failed
      And I delete the Slack notification "thread"
      Then I received exactly one slack_bridge_pending_actions.changed event, about the action it created

  Rule: Without the extension bridge

    @legacy:test_realtime_events::slack_bridge::test_delete_slack_notification_with_bridge_disabled_publishes_no_pending_actions_event
    Scenario: Deleting a Slack notification announces no action when the bridge is disabled
      Given I am connected to Slack, syncing its notifications, with the extension bridge disabled
      And I have the Slack notification "thread"
      And I listen to the realtime events of my user
      When I delete the Slack notification "thread"
      Then I received no slack_bridge_pending_actions.changed event, and no action was created
