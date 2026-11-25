Feature: Changing Todoist tasks
  Todoist owns the tasks it hosts: a change made here reaches Todoist first,
  and a task that leaves the Inbox loses its notification. A task can also be
  created from a notification, or linked to one.

  Background:
    Given I am signed in

  Rule: A change to a task reaches Todoist

    Background:
      Given I am connected to Todoist
      And Todoist can list my projects
      And I have a Todoist task, added in the year 2000, in the project Inbox
      And the notification of that task is unread

    @legacy:test_todoist_tasks::patch_task::test_patch_todoist_task_status_as_deleted
    Scenario: Deleting a task deletes it in Todoist and deletes its notification
      Given Todoist deletes that task
      And that Todoist task is active
      When I mark that task as deleted
      Then the patched task is deleted
      And that Todoist task is deleted
      And the notification of that task is deleted

    @legacy:test_todoist_tasks::patch_task::test_patch_todoist_task_status_as_done
    Scenario: Completing a task completes it in Todoist and deletes its notification
      Given Todoist completes the task that I have
      And that Todoist task is active
      When I mark that task as done
      Then the patched task is done
      And the patched task has a completion date
      And that Todoist task is done
      And the notification of that task is deleted

    @legacy:test_todoist_tasks::patch_task::test_patch_todoist_task_to_plan_to_new_project
    Scenario: Planning a task moves it to a new project and updates its due date and priority
      Given that task is still in the project "Inbox", due "2016-09-01", with the lowest priority
      And Todoist creates the project "Project1" with the id "3333"
      And Todoist moves that task to the new project, and updates it with the due date "2022-01-01" and the priority P2
      When I move that task to the project "Project1", with the due date "2022-01-01" and the priority P2, and keep it active
      Then the patched task is in the project "Project1", due "2022-01-01", with the priority P2
      And the notification of that task is deleted

    @legacy:test_todoist_tasks::patch_task::test_patch_todoist_task_title_only
    Scenario: A rename alone reaches Todoist, with nothing else riding along
      Given Todoist renames that task to "Reply to Marc about the billing thread", in exactly one call that changes nothing else
      When I rename that task to "Reply to Marc about the billing thread"
      Then the patched task is titled "Reply to Marc about the billing thread", and so is that task when I read it again
      And the notification of that task is still unread

    @legacy:test_todoist_tasks::patch_task::test_patch_todoist_task_title_unchanged_is_a_no_op
    Scenario: Renaming a task to the title it already has changes nothing and reaches no provider
      Given Todoist is not asked to change anything
      When I rename that task to the title that it already has
      Then the app answers 304
      And the notification of that task is still unread

  Rule: A task is created from a notification

    Background:
      Given I am connected to GitHub
      And I have a GitHub notification
      And GitHub accepts to mark that notification as done
      And Todoist can list my projects

    @legacy:test_todoist_tasks::patch_task::test_create_todoist_task_from_notification
    Scenario: A task created with details is added to Todoist and takes the place of the notification
      Given I am connected to Todoist
      And Todoist adds the item in the project Project2, as "Release new version of Universal Inbox", and then serves it
      When I create a task from that notification, with the title, the due date and the priority of the Todoist item, in the project Project2
      Then the task that I created is the Todoist item in the project Project2, and its notification is deleted and was read just now
      And the notification stays linked to the task that I created

    @legacy:test_todoist_tasks::patch_task::test_create_todoist_task_with_defaults_from_notification
    Scenario: A task created without details takes the defaults of the connection
      Given I am connected to Todoist, with the default project Project2, the due date today and the priority of the Todoist item
      And Todoist adds the item with the defaults of my connection, as the title of that notification, and then serves it
      When I create a task from that notification, without any detail
      Then the task that I created is the Todoist item in the project Project2, and its notification is deleted and was read just now
      And the notification stays linked to the task that I created

  Rule: A notification is linked to an existing task

    @legacy:test_todoist_tasks::patch_notification::test_patch_notification_to_link_with_task
    Scenario: Linking a notification to a task adds a link to the notification in the description of the task
      Given I am connected to GitHub
      And I have a GitHub notification
      And I am connected to Todoist
      And Todoist can list my projects
      And I have a Todoist task, added in the year 2000, in the project Inbox
      And the notification of that task is unread
      And Todoist is told to add a link to that notification to the description of that task
      When I link that notification to that task
      Then that notification is linked to that task, when I read it again
