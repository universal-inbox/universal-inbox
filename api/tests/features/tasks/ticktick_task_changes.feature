Feature: Changing TickTick tasks
  TickTick owns the tasks it hosts: a change made here reaches TickTick first, and a task
  that leaves the Inbox loses its notification. A task can also be created from a
  notification, with or without a time of the day, or linked to one.

  Background:
    Given I am signed in
    And I am connected to TickTick, syncing its tasks

  Rule: A change to a task reaches TickTick

    Background:
      Given I have a TickTick task in my Inbox
      And the notification of that TickTick task is unread

    @legacy:test_ticktick_tasks::patch_task::test_patch_ticktick_task_status_as_deleted
    Scenario: Deleting a task deletes it in TickTick and deletes its notification
      Given TickTick deletes that task, once
      When I mark that TickTick task as deleted
      Then the patched TickTick task is deleted
      And the notification of that TickTick task is deleted

    @legacy:test_ticktick_tasks::patch_task::test_patch_ticktick_task_status_as_done
    Scenario: Completing a task completes it in TickTick and deletes its notification
      Given TickTick completes that task, once
      When I mark that TickTick task as done
      Then the patched TickTick task is done
      And the patched TickTick task has a completion date
      And the notification of that TickTick task is deleted

    @legacy:test_ticktick_tasks::patch_task::test_patch_ticktick_task_title_only
    Scenario: A rename alone reaches TickTick, with the real project id
      Given TickTick renames that task to "Reply to Marc about the billing thread", once, with its real project id
      When I rename that TickTick task to "Reply to Marc about the billing thread"
      Then the patched TickTick task is titled "Reply to Marc about the billing thread", and so is that task when I read it again
      And the notification of that TickTick task is still unread

  Rule: A task is created from a notification

    Background:
      Given I am connected to GitHub
      And I have a GitHub notification
      And GitHub accepts to mark that notification as done

    @legacy:test_ticktick_tasks::patch_task::test_create_ticktick_task_from_notification
    Scenario: A task created from a notification is added to TickTick and takes the place of the notification
      Given TickTick creates the item in the project Project2, as the Universal Inbox item, and then serves it
      When I create a TickTick task from that notification, in the project Project2
      Then the task that I created is the TickTick item in the project Project2, and its notification is deleted and was read just now
      And the notification stays linked to the TickTick task that I created

    @legacy:test_ticktick_tasks::patch_task::test_create_ticktick_task_from_notification_with_time_config
    Scenario: A task created with a time of the day reaches TickTick with its schedule
      Given TickTick creates the item in the project Project2, from 12:00 to 12:30 UTC on 2026-06-29, in the time zone Europe/Paris, and then serves it
      When I create a TickTick task from that notification, in the project Project2, due at 12:00 UTC on 2026-06-29, at 14:00 for 30 minutes in Europe/Paris
      Then the task that I created is the TickTick item in the project Project2, and its notification is deleted and was read just now
      And the notification stays linked to the TickTick task that I created

  Rule: A notification is linked to an existing task

    @legacy:test_ticktick_tasks::patch_task::test_link_notification_with_ticktick_task_sends_real_project_id
    Scenario: Linking a notification adds its title to the body of the task, sent with the real project id
      Given I am connected to GitHub
      And I have a GitHub notification
      And GitHub accepts to mark that notification as done
      And I have a TickTick task in the project Project2, with the body "existing body"
      And TickTick adds the notification title to the body of that task, once, with its real project id
      When I link that notification to that TickTick task, deleting it
      Then that TickTick task, read again, has the title of that notification in its body
