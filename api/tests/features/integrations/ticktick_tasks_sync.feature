Feature: Syncing TickTick tasks
  A sync brings in the tasks of every TickTick project: new tasks are created, known
  tasks are updated, the tasks that leave the Inbox lose their notification, and
  TickTick owns the title of a task mirrored from Linear.

  Background:
    Given I am signed in

  Scenario: A sync adds the new tasks and updates the known one
    Given I am connected to TickTick, syncing its tasks
    And I already have the task "old task 1" of the second TickTick task, in my Inbox
    And TickTick reports its tasks, with nothing in its inbox data
    When I sync my TickTick tasks
    Then every TickTick task became a task
    And the task I already had was updated from the second TickTick task
    And the notification of that task was deleted when it left my Inbox, and is still titled "old task 1"
    And the new task of the first TickTick task, which is in my Inbox, has an unread notification
    And my TickTick connection recorded the date of the sync and a completed tasks sync, without failure

  Scenario: Renaming the TickTick item of a Linear task renames the task
    Given I am connected to TickTick, syncing its tasks
    And I am connected to Linear, syncing its tasks into the project TickTick Project2, due today, to TickTick
    And I already have the task of the Linear issue, mirrored by a TickTick item
    And the user renamed that item "Reply to Marc about the billing thread" in TickTick
    When I sync my TickTick tasks
    Then the sync updated the task I already had, which is now titled "Reply to Marc about the billing thread"

  Scenario: A connection that is not validated yet is not synced
    Given I have a TickTick connection that is not validated yet
    And TickTick answers every request with a success and no data
    When I ask for a sync of my TickTick tasks
    Then TickTick accepts the sync request
    And TickTick received no request

  Scenario: A connection with its synchronization disabled is not synced
    Given I am connected to TickTick, with its synchronization disabled
    And TickTick answers every request with a success and no data
    When I ask for a sync of my TickTick tasks
    Then TickTick accepts the sync request
    And TickTick received no request
