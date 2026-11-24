Feature: Syncing GitHub notifications
  A sync brings in the threads GitHub reports, adds the new ones and updates the
  known ones, without undoing what the user did in Universal Inbox: reading,
  deleting, unsubscribing, snoozing and linking a task are never forwarded to
  GitHub, which keeps reporting the thread as unread.

  Background:
    Given I am signed in
    And I am connected to GitHub

  Rule: A sync adds and updates notifications

    @legacy:test_sync_github_notifications::test_sync_notifications_should_add_new_notification_and_update_existing_one
    Scenario: A sync adds the new threads and updates the known one without losing my triage
      Given I am connected to Todoist
      And Todoist can list my projects
      And GitHub reports the notifications 123 and 456
      And GitHub knows pull request 123
      And I already have the notification "existing" of the GitHub thread 456
      And I have a Todoist task in the project "Project2", linked to "existing"
      And I snooze "existing" until "2064-01-01T00:00:00Z"
      When I sync my GitHub notifications
      Then my notifications are the threads GitHub reported
      And they carry what GitHub reported, with the details of pull request 123
      And "existing" is read since "2014-11-07T23:02:45Z"
      And "existing" is still snoozed until "2064-01-01T00:00:00Z"
      And "existing" is still linked to my Todoist task
      And my GitHub connection recorded a completed sync, without failure

    @legacy:test_sync_github_notifications::test_sync_notifications_should_handle_pull_request_without_commits
    Scenario: A pull request without any commit does not fail the sync
      Given GitHub reports the notifications 123 and 456
      And GitHub knows pull request 123, which has no commit
      When I sync my GitHub notifications
      Then my notifications are the threads GitHub reported
      And they carry what GitHub reported, with the details of pull request 123
      And my GitHub connection recorded a completed sync, without failure

    @legacy:test_sync_github_notifications::test_sync_notifications_should_heal_undecodable_existing_third_party_item
    Scenario: Stored data that no longer decodes is overwritten by what GitHub reports
      Given GitHub reports the notifications 123 and 456
      And GitHub knows pull request 123
      And I already have the notification "mine" of the GitHub thread 456
      And the stored GitHub data of "mine" no longer decodes
      When I sync my GitHub notifications
      Then my notifications are the threads GitHub reported
      And "mine" is healed in place: its stored data is what GitHub reported, and it is read since "2014-11-07T23:02:45Z"

  Rule: A deleted notification brought back by new activity keeps the latest read marker

    @legacy:test_sync_github_notifications::test_sync_notifications_should_keep_latest_last_read_at_of_deleted_notification::case_1_github_marker_is_older
    Scenario: GitHub's read marker is older than the moment I deleted the notification
      Given GitHub reports the notifications 123 and 456
      And GitHub knows pull request 123
      And I already have the notification "mine" of the GitHub thread 456
      And I deleted "mine" in Universal Inbox
      And GitHub reports the thread 456 as unread again, with new activity and its former read marker
      When I sync my GitHub notifications
      Then "mine" is unread, its read marker being the moment I deleted it

    @legacy:test_sync_github_notifications::test_sync_notifications_should_keep_latest_last_read_at_of_deleted_notification::case_2_github_marker_is_newer
    Scenario: GitHub's read marker is newer than the moment I deleted the notification
      Given GitHub reports the notifications 123 and 456
      And GitHub knows pull request 123
      And I already have the notification "mine" of the GitHub thread 456
      And I deleted "mine" in Universal Inbox
      And GitHub reports the thread 456 as unread again, with new activity and a read marker one hour after I deleted it
      When I sync my GitHub notifications
      Then "mine" is unread, its read marker being one hour after I deleted it

  Rule: A sync of an unchanged thread does not undo my triage

    Scenario Outline: A thread I marked <status> stays <status>
      Given GitHub reports the notifications 123 and 456
      And GitHub reports every thread as unread
      And GitHub knows pull request 123
      When I sync my GitHub notifications
      And I take note of the synced notification of the GitHub thread 456 as "mine"
      And I change "mine" to <status> in Universal Inbox only
      And I force a new sync of my GitHub notifications, which reports the same threads
      Then "mine" keeps the status I gave it, its update time and its task

      @legacy:test_sync_github_notifications::test_sync_notifications_should_keep_user_status_of_unchanged_notification::case_1_read
      Examples:
        | status       |
        | read         |

      @legacy:test_sync_github_notifications::test_sync_notifications_should_keep_user_status_of_unchanged_notification::case_2_deleted
      Examples:
        | status       |
        | deleted      |

      @legacy:test_sync_github_notifications::test_sync_notifications_should_keep_user_status_of_unchanged_notification::case_3_unsubscribed
      Examples:
        | status       |
        | unsubscribed |

    @legacy:test_sync_github_notifications::test_sync_notifications_should_keep_user_status_of_unchanged_notification::case_4_read_linked_to_task
    Scenario: A thread I marked read and linked to a task stays read and linked
      Given GitHub reports the notifications 123 and 456
      And GitHub reports every thread as unread
      And GitHub knows pull request 123
      When I sync my GitHub notifications
      And I take note of the synced notification of the GitHub thread 456 as "mine"
      Given I am connected to Todoist
      Given Todoist can list my projects
      Given I have a Todoist task in the project "Project2", linked to "mine"
      When I change "mine" to read in Universal Inbox only
      And I force a new sync of my GitHub notifications, which reports the same threads
      Then "mine" keeps the status I gave it, its update time and its task
