Feature: Syncing Todoist tasks
  A sync brings in the items Todoist changed since the last sync token: new items
  become tasks, known tasks are updated, the tasks that leave the Inbox lose their
  notification, and a completed item completes its task. Syncs requested too soon
  do nothing, and a failing sync records its failure.

  Background:
    Given I am signed in

  Rule: Items become tasks

    Background:
      Given I am connected to Todoist
      And Todoist can list my projects

    @legacy:test_sync_todoist_tasks::test_sync_tasks_should_add_new_task_and_update_existing_one
    Scenario: A sync adds the new tasks and updates the known one
      Given I already have the task "old task 1" of the second Todoist item, in my Inbox
      And Todoist reports its items
      When I sync the tasks of my Todoist connection
      Then every Todoist item became a task
      And the task I already had was updated from the second Todoist item
      And its notification was deleted, as the task left my Inbox, and is still titled "old task 1"
      And the new task of the first Todoist item, which is in my Inbox, has an unread notification
      And my Todoist connection kept its configuration and recorded the sync token of that sync
      And my Todoist connection recorded a completed tasks sync, without failure

    @legacy:test_sync_todoist_tasks::test_sync_tasks_should_mark_as_completed_tasks_not_active_anymore
    Scenario: A completed item completes its task and deletes its notification
      Given I already have the tasks of every Todoist item, in my Inbox
      And Todoist reports its items
      And Todoist reports the item "1123" as completed
      When I sync the tasks of my Todoist connection
      Then every Todoist item became a task
      And the task of the Todoist item "1123" is done, and its notification deleted

    @legacy:test_sync_todoist_tasks::test_sync_tasks_should_not_update_tasks_and_notifications_with_empty_incremental_sync_response
    Scenario: An incremental sync leaves alone the tasks Todoist does not report
      Given I already have the tasks of every Todoist item, in my Inbox
      And Todoist reports its items, except the first one
      When I sync the tasks of my Todoist connection
      Then the sync returned the second task only, with a notification, and that task is active
      And the first task and its notification were not updated

    @legacy:test_sync_todoist_tasks::test_sync_tasks_should_reuse_existing_sync_token
    Scenario: A sync asks Todoist for the changes since the last sync token
      Given the sync token of my Todoist connection is "previous_sync_token"
      And Todoist reports no change since that sync token, and gives the new sync token "new_sync_token"
      When I sync the tasks of my Todoist connection
      Then the sync created no task
      And my Todoist connection kept its configuration and recorded the sync token of that sync

    @legacy:test_sync_todoist_tasks::test_sync_tasks_should_add_new_empty_task
    Scenario: An empty item attached to no project becomes a deleted task
      Given Todoist reports a single deleted item with no content, attached to no project
      When I sync the tasks of my Todoist connection
      Then that task is a deleted task with no content, attached to no project

  Rule: A connection that does not create notifications from the Inbox

    @legacy:test_sync_todoist_tasks::test_sync_tasks_should_add_new_task_and_delete_notification_when_disabling_notification_creation_from_inbox_tasks
    Scenario: No notification is created for the tasks of the Inbox
      Given I am connected to Todoist, without a notification for the tasks of my Inbox
      And Todoist can list my projects
      And Todoist reports its items
      When I sync the tasks of my Todoist connection
      Then the sync returned one task for each Todoist item
      And no notification was created

  Rule: A connection that cannot sync is left alone

    @legacy:test_sync_todoist_tasks::test_sync_tasks_with_no_validated_integration_connections
    Scenario: A connection that is not validated yet is not synced
      Given I have a Todoist connection that is not validated yet
      And Todoist answers every request with a success and no data
      When I ask for a synchronous sync of my Todoist tasks
      Then the sync request is accepted

    @legacy:test_sync_todoist_tasks::test_sync_tasks_with_synchronization_disabled
    Scenario: A connection with its synchronization disabled is not synced
      Given I am connected to Todoist, with its synchronization disabled
      And Todoist answers every request with a success and no data
      When I ask for a synchronous sync of my Todoist tasks
      Then the sync request is accepted

  Rule: Background syncs

    Background:
      Given I am connected to Todoist
      And Todoist can list my projects
      And I already have the task "old task 1" of the second Todoist item, in my Inbox
      And Todoist reports its items

    @legacy:test_sync_todoist_tasks::test_sync_all_tasks_asynchronously::case_1_trigger_sync_when_listing_tasks
    Scenario: Listing the tasks triggers a sync, which does not run again right away
      When I list my active tasks, which triggers a sync
      Then I have 1 active task
      When I wait until 2 active tasks are listed
      And I note how many requests Todoist received
      And Todoist answers every request with a success and no data
      And I ask for a background sync of my Todoist tasks
      Then the sync is accepted to run in the background
      Given a second passes
      When I list the active tasks I have
      Then I have 2 active tasks
      And Todoist received no other request

    @legacy:test_sync_todoist_tasks::test_sync_all_tasks_asynchronously::case_2_trigger_sync_with_sync_endpoint
    Scenario: The sync endpoint starts a background sync, which does not run again right away
      When I ask for a background sync of my Todoist tasks
      Then the sync is accepted to run in the background
      When I wait until 2 active tasks are listed
      And I note how many requests Todoist received
      And Todoist answers every request with a success and no data
      And I ask for a background sync of my Todoist tasks
      Then the sync is accepted to run in the background
      Given a second passes
      When I list the active tasks I have
      Then I have 2 active tasks
      And Todoist received no other request

  Rule: A failing background sync records its failure

    @legacy:test_sync_todoist_tasks::test_sync_all_tasks_asynchronously_in_error
    Scenario: A background sync that fails syncs nothing and records the failure
      Given I am connected to Todoist
      And Todoist answers every request with an error
      When I ask for a background sync of my Todoist tasks
      Then the sync is accepted to run in the background
      Given a second passes
      When I list the active tasks I have
      Then I have 0 active tasks
      And my Todoist connection recorded the failure "Failed to fetch tasks from Todoist"
