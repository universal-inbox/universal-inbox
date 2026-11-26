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

    Scenario: The default project and due date do not overwrite those of a known task
      Given I am connected to Linear, syncing its tasks into the project Project2, due tomorrow
      And I already have the task of that issue in the project Project1, mirrored by Todoist
      And a second passes, so that what follows is dated after what exists
      And Linear reports my assigned issue
      When I sync my Linear tasks
      Then the sync updated the task I already had
      And that task kept its project and its due date

    Scenario: Retitling the issue changes neither the title of a mirrored task nor Todoist
      Given I am connected to Linear, syncing its tasks into the project Project2, due today
      And I already have the task of that issue in the project Project2, mirrored by Todoist
      And Todoist accepts no call from the sync
      And a second passes, so that what follows is dated after what exists
      And Linear reports that issue retitled "Renamed upstream in Linear"
      When I sync my Linear tasks
      Then the sync updated the task I already had
      And that task kept its title
      And Todoist received no sync request

    Scenario: Retitling the issue does not change the title of an unmirrored task either
      Given I am connected to Linear, syncing its tasks into the project Project2, due today
      And I already have the task of that issue in the project Project2, due today, not mirrored by any task manager
      And Todoist accepts no call from the sync
      And a second passes, so that what follows is dated after what exists
      And Linear reports that issue retitled "Renamed upstream in Linear"
      When I sync my Linear tasks
      Then the sync updated the task I already had
      And that task kept its title
      And Todoist received no sync request

    Scenario: An issue that is no longer assigned completes its task
      Given I am connected to Linear, syncing its tasks into the project Project2, due no date
      And I already have the task of that issue in the project Project2, mirrored by Todoist
      And Todoist completes the item of that task
      And Linear reports no assigned issue
      When I sync my Linear tasks
      Then the sync returned one task, without any notification
      And the sync updated the task I already had
      And that task is done

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

    Scenario: A Todoist item recreated after an update keeps the title the user gave
      Given I am connected to Linear, syncing its tasks into the project Project2, due no date
      And I already have the task of that issue in the project Project2, mirrored by Todoist
      And I renamed that task "Reply to Marc about the billing thread" in Todoist
      And a second passes, so that what follows is dated after what exists
      And Linear reports that issue with the description "The issue description changed upstream"
      And Todoist no longer knows the item of that task, so updating its description fails
      And Todoist creates the item "another_id" for that task, with its title and the description Linear reports
      When I sync my Linear tasks
      Then the sync updated the task I already had
      And that task is titled "Reply to Marc about the billing thread"
      And that task is mirrored by a new Todoist item "another_id", not the one it had

    Scenario: Updating an unmirrored task creates its Todoist item
      Given I am connected to Linear, syncing its tasks into the project Project2, due no date
      And I already have the task of that issue in the project Project2, due today, not mirrored by any task manager
      And a second passes, so that what follows is dated after what exists
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

    Scenario: Syncing the task deletes the notification of its issue
      Given I already have the Linear notification of that issue, "mine", which I have read
      And Linear reports my assigned issue
      And Todoist creates the item "new_item" for the new task, titled with a link to the issue
      When I sync my Linear tasks
      Then my Linear notification "mine" is deleted

    Scenario: Syncing the task deletes the notification of the syncing user only
      Given another user is signed in
      And that other user is connected to Linear, syncing its tasks into the project Project2, due today
      And that other user has the Linear notification of that issue, "theirs", which they have read
      And I already have the Linear notification of that issue, "mine", which I have read
      And Linear reports my assigned issue
      And Todoist creates the item "new_item" for the new task, titled with a link to the issue
      When I sync my Linear tasks
      Then my Linear notification "mine" is deleted
      And the Linear notification "theirs" of that other user is still read
      And that other user has no task

    Scenario: The notification of an issue that already has a task is created deleted
      Given I already have the task of that issue in the project Project2, mirrored by Todoist
      When the Linear notification of that issue arrives as "mine"
      Then my Linear notification "mine" is deleted
      And the status of "mine" never changed

    Scenario: Syncing the notification again keeps it deleted, without flipping its status
      Given I already have the task of that issue in the project Project2, mirrored by Todoist
      And I received the Linear notification of that issue, "mine", which was created already deleted
      When I sync my Linear notifications
      Then my Linear notification "mine" is deleted
      And the status of "mine" never changed

    Scenario: New unread activity does not bring the deleted notification back
      Given I already have the task of that issue in the project Project2, mirrored by Todoist
      And I received the Linear notification of that issue, "mine", which was created already deleted
      And Linear reports new unread activity on that notification
      When I sync my Linear notifications
      Then my Linear notification "mine" is deleted
      And the status of "mine" never changed

    Scenario: Once the task is deleted the notification follows Linear again
      Given I already have the task of that issue in the project Project2, mirrored by Todoist
      And I received the Linear notification of that issue, "mine", which was created already deleted
      And I deleted that task
      When I sync my Linear notifications
      Then my Linear notification "mine" is read

  Rule: The task manager is chosen by the Linear configuration, then by the user's default

    Background:
      Given I am connected to TickTick

    Scenario: The Linear configuration sends the task to TickTick
      Given I am connected to Linear, syncing its tasks into the project TickTick Project2, due today, to TickTick
      And Linear reports my assigned issue
      And TickTick creates the sink item of the new task
      When I sync my Linear tasks
      Then that task is mirrored by a TickTick item

    Scenario: Without a choice in the configuration the user's default task manager is used
      Given I am connected to Linear, syncing its tasks into the project TickTick Project2, due today
      And TickTick is my default task manager
      And Linear reports my assigned issue
      And TickTick creates the sink item of the new task
      When I sync my Linear tasks
      Then that task is mirrored by a TickTick item
