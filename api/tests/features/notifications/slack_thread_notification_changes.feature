Feature: Changing the notification of a Slack thread
  The API of Slack can neither mark a thread as read nor stop following it, so the app
  records the change in its own copy of the thread and asks Slack nothing. A snooze stays in
  the app.

  Background:
    Given I am signed in
    And I have a Slack connection that syncs notifications, and the notification "mine" of a Slack thread that I follow and have not read

  @legacy:test_slack_notifications::patch_resource_slack_thread::test_patch_slack_notification_status_as_deleted
  Scenario: Deleting the notification marks the thread as read up to its last message
    Given Slack is not asked anything
    When I set the status of "mine" to deleted
    Then the patched notification is deleted, was read just now, and is otherwise as "mine" was
    And the Slack thread of "mine", read again, is read up to its last message, and still followed

  @legacy:test_slack_notifications::patch_resource_slack_thread::test_patch_slack_notification_status_as_unsubscribed
  Scenario: Unsubscribing from the notification stops following the thread
    Given Slack is not asked anything
    When I set the status of "mine" to unsubscribed
    Then the patched notification is unsubscribed, was read just now, and is otherwise as "mine" was
    And the Slack thread of "mine", read again, is no longer followed, and still not read

  @legacy:test_slack_notifications::patch_resource_slack_thread::test_patch_slack_notification_snoozed_until
  Scenario: Snoozing the notification leaves the thread as it was
    Given Slack is not asked anything
    When I snooze "mine" until "2022-01-01T01:02:03Z"
    Then the patched notification is "mine" snoozed until "2022-01-01T01:02:03Z"
    And the Slack thread of "mine", read again, is still followed and not read

  Scenario Outline: Another user cannot set my notification to <status>, and their notification of the same thread is left alone
    Given another user is signed in
    And that other user has their own Slack connection, and their own notification "theirs" of the same Slack thread
    And Slack is not asked anything
    When that other user tries to change the status of "mine" to <status>
    Then the app answers 403
    And "mine", read again, is exactly as it was
    And the notification "theirs" of that other user, read by that other user, is exactly as it was

    Examples:
      | status       |
      | deleted      |
      | unsubscribed |
