Feature: Syncing Linear tasks
  A sync turns the issues Linear assigns to the user into tasks mirrored in their
  task manager. Linear only seeds a task: it never overwrites the title, the due
  date or the project the user owns, and an issue that is no longer assigned
  completes its task.

  Background:
    Given I am signed in

  Rule: Tasks are created and updated in Todoist

    Background:
      Given I am connected to Todoist
      And Todoist can list my projects

    @legacy:test_sync_linear_tasks::test_sync_tasks_should_create_new_task
    Scenario: A new issue becomes a task mirrored in Todoist
      Given I am connected to Linear, syncing its tasks into the project Project2, due today
      And Linear reports my assigned issue
      And Todoist creates the item "new_item" for the new task, titled with a link to the issue
      When I sync my Linear tasks
      Then the sync returned one task, without any notification
      And that task comes from the Linear issue, titled with a link to it and carrying its description
      And that task is active, in the project Project2, due today
      And that task is mirrored by the Todoist item "new_item"
      And my Linear connection recorded a completed tasks sync, without failure

    @legacy:test_sync_linear_tasks::test_sync_tasks_should_not_update_default_values
    Scenario: The default project and due date do not overwrite those of a known task
      Given I am connected to Linear, syncing its tasks into the project Project2, due tomorrow
      And I already have the task of that issue in the project Project1, mirrored by Todoist
      And a second passes
      And Linear reports my assigned issue
      When I sync my Linear tasks
      Then the sync updated the task I already had
      And that task kept its project and its due date

    @legacy:test_sync_linear_tasks::test_sync_tasks_should_not_overwrite_nor_push_the_title_of_a_mirrored_task
    Scenario: Retitling the issue changes neither the title of a mirrored task nor Todoist
      Given I am connected to Linear, syncing its tasks into the project Project2, due today
      And I already have the task of that issue in the project Project2, mirrored by Todoist
      And Todoist accepts no call from the sync
      And a second passes
      And Linear reports that issue retitled "Renamed upstream in Linear"
      When I sync my Linear tasks
      Then the sync updated the task I already had
      And that task kept its title
      And Todoist received no sync request

    @legacy:test_sync_linear_tasks::test_sync_tasks_should_not_overwrite_the_title_of_an_unmirrored_task
    Scenario: Retitling the issue does not change the title of an unmirrored task either
      Given I am connected to Linear, syncing its tasks into the project Project2, due today
      And I already have the task of that issue in the project Project2, not mirrored by any task manager
      And Todoist accepts no call from the sync
      And a second passes
      And Linear reports that issue retitled "Renamed upstream in Linear"
      When I sync my Linear tasks
      Then the sync updated the task I already had
      And that task kept its title
      And Todoist received no sync request

    @legacy:test_sync_linear_tasks::test_sync_tasks_should_complete_existing_task
    Scenario: An issue that is no longer assigned completes its task
      Given I am connected to Linear, syncing its tasks into the project Project2, due no date
      And I already have the task of that issue in the project Project2, mirrored by Todoist
      And Todoist completes the item of that task
      And Linear reports no assigned issue
      When I sync my Linear tasks
      Then the sync returned one task, without any notification
      And the sync updated the task I already had
      And that task is done

    @legacy:test_sync_linear_tasks::test_sync_tasks_should_complete_existing_task_and_recreate_sink_task_if_upstream_task_does_not_exist_anymore
    Scenario: Completing a task whose Todoist item is gone recreates the item
      Given I am connected to Linear, syncing its tasks into the project Project2, due no date
      And I already have the task of that issue in the project Project2, mirrored by Todoist
      And Todoist no longer knows the item of that task, so completing it fails
      And Todoist creates the item "another_id" for that task, with its title and the description Linear reports
      And Linear reports no assigned issue
      When I sync my Linear tasks
      Then the sync returned one task, without any notification
      And the sync updated the task I already had
      And that task is done
      And that task is mirrored by a new Todoist item "another_id", not the one it had

    @legacy:test_sync_linear_tasks::test_sync_tasks_should_recreate_sink_task_with_the_renamed_title
    Scenario: A Todoist item recreated after an update keeps the title the user gave
      Given I am connected to Linear, syncing its tasks into the project Project2, due no date
      And I already have the task of that issue in the project Project2, mirrored by Todoist
      And I renamed that task "Reply to Marc about the billing thread" in Todoist
      And a second passes
      And Linear reports that issue with the description "The issue description changed upstream"
      And Todoist no longer knows the item of that task, so updating its description fails
      And Todoist creates the item "another_id" for that task, with its title and the description Linear reports
      When I sync my Linear tasks
      Then the sync updated the task I already had
      And that task is titled "Reply to Marc about the billing thread"
      And that task is mirrored by a new Todoist item "another_id", not the one it had

    @legacy:test_sync_linear_tasks::test_sync_tasks_should_create_sink_item_if_missing_when_updating_task
    Scenario: Updating an unmirrored task creates its Todoist item
      Given I am connected to Linear, syncing its tasks into the project Project2, due no date
      And I already have the task of that issue in the project Project2, not mirrored by any task manager
      And a second passes
      And Linear reports that issue with the description "This issue was edited upstream"
      And Todoist creates the item "new_todoist_id" for that task, with its title and the description Linear reports
      When I sync my Linear tasks
      Then the sync updated the task I already had
      And that task is mirrored by a new Todoist item "new_todoist_id", not the one it had

  Rule: The notification of an issue synced as a task is deleted

    Background:
      Given I am connected to Todoist
      And Todoist can list my projects
      And I am connected to Linear, syncing its tasks into the project Project2, due today

    @legacy:test_sync_linear_tasks::test_sync_tasks_should_auto_delete_existing_linear_notification
    Scenario: Syncing the task deletes the notification of its issue
      Given I already have the Linear notification of that issue, "mine", which I have read
      And Linear reports my assigned issue
      And Todoist creates the item "new_item" for the new task, titled with a link to the issue
      When I sync my Linear tasks
      Then my Linear notification "mine" is deleted

    @legacy:test_sync_linear_tasks::test_create_notification_should_auto_delete_when_task_already_exists
    Scenario: The notification of an issue that already has a task is created deleted
      Given I already have the task of that issue in the project Project2, mirrored by Todoist
      And I received the Linear notification of that issue, "mine", which was created already deleted
      Then the status of "mine" never changed

    @legacy:test_sync_linear_tasks::resync_of_notification_for_issue_synced_as_task::test_resync_should_keep_notification_deleted_without_status_change
    Scenario: Syncing the notification again keeps it deleted, without flipping its status
      Given I already have the task of that issue in the project Project2, mirrored by Todoist
      And I received the Linear notification of that issue, "mine", which was created already deleted
      When I sync my Linear notifications
      Then my Linear notification "mine" is deleted
      And the status of "mine" never changed

    @legacy:test_sync_linear_tasks::resync_of_notification_for_issue_synced_as_task::test_resync_with_new_activity_should_keep_notification_deleted
    Scenario: New unread activity does not bring the deleted notification back
      Given I already have the task of that issue in the project Project2, mirrored by Todoist
      And I received the Linear notification of that issue, "mine", which was created already deleted
      And Linear reports new unread activity on that notification
      When I sync my Linear notifications
      Then my Linear notification "mine" is deleted
      And the status of "mine" never changed

    @legacy:test_sync_linear_tasks::resync_of_notification_for_issue_synced_as_task::test_resync_after_task_deletion_should_use_linear_status
    Scenario: Once the task is deleted the notification follows Linear again
      Given I already have the task of that issue in the project Project2, mirrored by Todoist
      And I received the Linear notification of that issue, "mine", which was created already deleted
      And I deleted that task
      When I sync my Linear notifications
      Then my Linear notification "mine" is read

  Rule: The task manager is chosen by the Linear configuration, then by the user's default

    Background:
      Given I am connected to TickTick

    @legacy:test_sync_linear_tasks::sink_provider_resolution::test_sync_linear_task_routes_to_ticktick_when_config_says_so
    Scenario: The Linear configuration sends the task to TickTick
      Given I am connected to Linear, syncing its tasks into the project TickTick Project2, due today, to TickTick
      And Linear reports my assigned issue
      And TickTick creates the sink item of the new task
      When I sync my Linear tasks
      Then that task is mirrored by a TickTick item

    @legacy:test_sync_linear_tasks::sink_provider_resolution::test_sync_linear_task_falls_back_to_user_default_task_manager
    Scenario: Without a choice in the configuration the user's default task manager is used
      Given I am connected to Linear, syncing its tasks into the project TickTick Project2, due today
      And TickTick is my default task manager
      And Linear reports my assigned issue
      And TickTick creates the sink item of the new task
      When I sync my Linear tasks
      Then that task is mirrored by a TickTick item
