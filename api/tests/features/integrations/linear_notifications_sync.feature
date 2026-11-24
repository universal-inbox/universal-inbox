Feature: Syncing Linear notifications
  A sync brings in the notifications Linear reports, updates the known ones without
  losing what the user did in Universal Inbox, and deletes the older notifications
  of an issue that has a newer one.

  Background:
    Given I am signed in
    And I am connected to Linear

  @legacy:test_sync_linear_notifications::test_sync_notifications_should_add_new_notification_and_update_existing_one
  Scenario: A sync adds the new notifications and updates the known one
    Given I am connected to Todoist
    And Todoist can list my projects
    And I already have the Linear notification of that issue, "existing", which I have read
    And I have a Todoist task in the project "Project2", linked to "existing"
    And the snooze of "existing" until "2064-01-01T00:00:00Z" is stored directly, as Linear does not allow it from the API
    When I sync my Linear notifications
    Then Linear reported comments parsed with only their parent comment and its answers
    And my notifications are the ones Linear reported
    And "existing" now follows what Linear reported: read since "2023-07-31T06:01:27.112Z", snoozed until "2023-08-12T07:00:00Z"
    And "existing" is still linked to my Todoist task
    And my Linear connection recorded a completed notifications sync, without failure

  @legacy:test_sync_linear_notifications::test_sync_linear_notifications_should_mark_existing_notifications_for_same_issue_as_deleted
  Scenario: An older notification of an issue is deleted when a newer one arrives
    Given I already have the Linear notification of that issue, "mine", which I have read
    And Linear reports a second notification of that issue, with a newer comment
    When I sync my Linear notifications
    Then the sync returned 5 Linear notifications, as two of them are about the same issue
    And my Linear notification "mine" is deleted
    And the newer notification of that issue is read

  @legacy:test_sync_linear_notifications::test_sync_notifications_should_keep_user_status_of_unchanged_notification
  Scenario: Reading a notification in Universal Inbox survives a sync that reports it unchanged
    When I sync my Linear notifications
    And I take note of the unread notification Linear reported as "mine"
    And I read "mine" in Universal Inbox only, which was last updated at "2026-10-21T12:57:31Z"
    And I force a new sync of my Linear notifications, which reports the same notifications
    Then "mine" is still read, and was last updated at "2026-10-21T12:57:31Z"
