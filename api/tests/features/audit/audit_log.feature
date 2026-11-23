Feature: The audit log
  Every change to a notification or a task is recorded with who made it, what
  changed and, for a bulk action, the transaction it shares. A user lists only
  their own events, in pages, and events older than the retention are purged.

  Background:
    Given I am signed in

  Rule: Who made a change

    @legacy:test_audit_log::actors::test_web_user_change_is_recorded_with_its_from_and_to_values
    Scenario: A change made on the web records what it changed
      Given I am connected to GitHub
      And I have a GitHub notification called "mine"
      When I snooze "mine" until "2064-01-01T09:00:00Z"
      And I list the audit events of the notification "mine"
      Then the newest event is an update of the notification "mine" on GitHub by me through the web, snoozing it until "2064-01-01T09:00:00Z" from no snooze
      And the oldest event is the creation of the notification, with the status going from nothing to "Unread"

    @legacy:test_audit_log::actors::test_api_key_and_extension_changes_are_told_apart
    Scenario: A change with an API key and one with the extension are told apart
      Given I am connected to GitHub
      And I have a GitHub notification called "mine"
      And I have an API token
      When I snooze "mine" until "2064-01-01T09:00:00Z" with my API token
      And I snooze "mine" until "2065-01-01T09:00:00Z" with my API token, as the extension in version "1.4.2"
      And I list the audit events of the notification "mine" that snoozed it
      Then the actors of the events are, newest first:
        | extension 1.4.2 |
        | API key         |

    @legacy:test_audit_log::actors::test_sync_changes_are_recorded_as_a_system_job_triggered_by_the_user
    Scenario: A sync is recorded as a system job triggered by the user
      Given I am connected to Todoist
      And Todoist can list my projects
      And Todoist has its items to sync
      When I sync my Todoist tasks
      And I list my audit events with the query "entity_kind=Task&change=Created"
      Then every listed event is a creation by the SyncTasks job for Todoist, triggered by me through the web

  Rule: What a change records

    @legacy:test_audit_log::changes::test_bulk_change_events_share_their_transaction
    Scenario: The events of a bulk change share their transaction
      Given I am connected to GitHub
      And GitHub accepts to delete the notification threads 1 and 2
      And I have a GitHub notification called "first"
      And I have a GitHub notification called "second"
      When I delete all my unread GitHub notifications
      And I list my audit events with the query "change=Status&actor_kinds=User"
      Then 2 audit events are listed in total
      And the first two events share their transaction, and each changes the status from "Unread" to "Deleted"

    @legacy:test_audit_log::changes::test_task_creation_records_its_parameters_but_not_its_body
    Scenario: A task creation records its parameters but never the task body
      Given I am connected to GitHub
      And I have a GitHub notification called "mine"
      And I am connected to Todoist
      And GitHub accepts to delete the notification thread 1
      And Todoist can list my projects
      And Todoist accepts a task for the notification "mine" in the project "Project2", whose description must stay secret
      When I create the task from the notification "mine" in the project "Project2"
      Then the audit events of the task do not contain the secret description, which is the task body
      And the task creation event is by me through the web and records the title, the project "Project2", the kind "Todoist" and that a body is present
      And the history of the task includes the notification "mine", linked to the task

  Rule: Listing the events

    @legacy:test_audit_log::listing::test_events_of_another_user_are_never_listed
    Scenario: A user never sees the events of another user
      Given I am connected to GitHub
      And I have a GitHub notification called "mine"
      And another user is signed in
      When that other user lists their audit events with the query ""
      Then 0 audit events are listed in total
      When that other user lists the audit events of the notification "mine"
      Then 0 audit events are listed in total

    @legacy:test_audit_log::listing::test_events_are_paged_and_filtered_by_title
    Scenario: Events are paged and filtered by title
      Given I am connected to GitHub
      And I have a GitHub notification called "mine"
      When I snooze "mine" until "2064-01-01T09:00:00Z"
      And I snooze "mine" until "2064-01-02T09:00:00Z"
      And I snooze "mine" until "2064-01-03T09:00:00Z"
      And I list my audit events with the query "per_page=2"
      Then 4 audit events are listed in total
      And the page holds 2 events and links to the next page at the offset 2
      When I list the next page of my audit events with the query "per_page=2"
      Then the second page holds 2 events, has no next page and ends with the creation of the notification
      When I search my audit events for the first word of the notification "mine", in lowercase
      Then 4 audit events are listed in total
      When I search my audit events for a title that does not exist
      Then 0 audit events are listed in total

    @legacy:test_audit_log::listing::test_groups_are_paged_without_splitting_a_bulk_action
    Scenario: Groups are paged without splitting a bulk action
      Given I am connected to GitHub
      And I have a GitHub notification called "first"
      And I have a GitHub notification called "second"
      And I have a GitHub notification called "third"
      And GitHub accepts to delete the notification threads 1, 2 and 3
      When I snooze "first" until "2064-01-01T09:00:00Z"
      And I snooze "first" until "2064-01-02T09:00:00Z"
      And I snooze "first" until "2064-01-03T09:00:00Z"
      And I delete all my unread GitHub notifications
      And I list my audit events with the query "actor_kinds=User"
      Then 6 audit events are listed in total
      When I list my audit event groups with the query "actor_kinds=User&per_page=2"
      Then the groups page holds 2 of 4 groups over 2 pages and links to the next page at the offset 2
      And the first group is the bulk deletion of 3 events sharing their transaction, each changing the status from "Unread" to "Deleted"
      When I list the next page of my audit event groups with the query "actor_kinds=User&per_page=2"
      Then the second groups page holds 2 groups and no next page, and the three snoozes follow, newest first

  Rule: Purging the expired events

    @legacy:test_audit_log::retention::test_purge_deletes_only_expired_events
    Scenario: Only the events past the retention are purged
      Given I am connected to GitHub
      And I have a GitHub notification called "mine"
      When I snooze "mine" until "2064-01-01T09:00:00Z"
      And the creation events grow 8 days old, which is past the 7 days of retention
      And the expired audit events are purged
      Then 1 audit event was deleted
      When I list my audit events with the query ""
      Then the only audit event left is an update
      When I list the audit events of an id nobody uses
      Then 0 audit events are listed in total
