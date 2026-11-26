Feature: A Linear task follows its item in Todoist
  A Linear issue synced as a task is mirrored by an item in Todoist, the task manager of the
  user. What the user does to that item comes back with the sync of Todoist: completing or
  reopening it moves the issue in Linear, and renaming it renames the task.

  Background:
    Given I am signed in
    And I am connected to Todoist
    And Todoist can list my projects

  Rule: Completing or reopening the item in Todoist moves the issue in Linear

    Background:
      Given I am connected to Linear, syncing its tasks into the Inbox of Todoist, due today

    Scenario Outline: The item, in the Inbox of Todoist, is reported <after> while the issue is <before> in Linear
      Given I have the task of a Linear issue that is <before> in Linear, mirrored by the item that Todoist reports
      And Todoist then reports that item as <after>, in the project "1111"
      And Linear moves that issue to its <after> state, once and with my access token
      When I sync the tasks of my Todoist connection
      Then the sync brought two tasks, each with one notification
      And the task of that Linear issue, read again, is <after>, and still the same task
      And the task of that Linear issue has one Todoist notification, which is <notification>

      @legacy:test_linear_tasks::test_sync_todoist_linear_task::case_1_with_completed_task_in_inbox
      Examples:
        | before | after | notification |
        | active | done  | deleted      |

      @legacy:test_linear_tasks::test_sync_todoist_linear_task::case_3_with_uncompleted_task_in_inbox
      Examples:
        | before | after  | notification |
        | done   | active | unread       |

    Scenario Outline: The item, in another project of Todoist, is reported <after> while the issue is <before> in Linear
      Given I have the task of a Linear issue that is <before> in Linear, mirrored by the item that Todoist reports
      And Todoist then reports that item as <after>, in the project "2222"
      And Linear moves that issue to its <after> state, once and with my access token
      When I sync the tasks of my Todoist connection
      Then the sync brought two tasks
      And the task of that Linear issue, read again, is <after>, and still the same task
      And the task of that Linear issue has no notification

      @legacy:test_linear_tasks::test_sync_todoist_linear_task::case_2_with_completed_task_not_in_inbox
      Examples:
        | before | after |
        | active | done  |

      @legacy:test_linear_tasks::test_sync_todoist_linear_task::case_4_with_uncompleted_task_not_in_inbox
      Examples:
        | before | after  |
        | done   | active |

    Scenario: My sync does not touch the task that another user has for the same issue and the same Todoist item id
      Given I have the task of a Linear issue that is active in Linear, mirrored by the item that Todoist reports
      And another user is signed in
      And that other user has connections to Todoist and Linear of their own, and their own active task of the same Linear issue, mirrored by a Todoist item with the same id
      And Todoist then reports that item as done, in the project "2222"
      And Linear moves that issue to its done state, once and with my access token
      And Linear is not asked anything more
      When I sync the tasks of my Todoist connection
      Then the task of that Linear issue, read again, is done, and still the same task
      And the sync brought back only tasks of mine
      And the task of that other user, read by that other user, is exactly as it was

  Rule: Renaming the item in Todoist renames the task

    @legacy:test_linear_tasks::test_sync_todoist_linear_task_should_adopt_the_sink_rename
    Scenario: The title that the user gave to the item in Todoist is adopted by the task
      Given I am connected to Linear, syncing its tasks into the project Project2, due today
      And I already have the task of that issue in the project Project2, mirrored by Todoist
      And Todoist reports only the item that mirrors that task, renamed "Reply to Marc about the billing thread"
      When I sync the tasks of my Todoist connection
      Then the sync brought back the task of that Linear issue, alone
      And the task of that Linear issue, read again, is titled "Reply to Marc about the billing thread"
