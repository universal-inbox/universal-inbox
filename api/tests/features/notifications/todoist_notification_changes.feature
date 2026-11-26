Feature: Changing the notification of a Todoist task
  A task of the Todoist Inbox comes with a notification. Deleting that notification deletes
  the task, in Todoist too. A snooze stays in the app. The status of such a notification
  cannot be changed in any other way: the task has to be moved out of the Inbox.

  Background:
    Given I am signed in
    And I am connected to Todoist
    And Todoist can list my projects

  Scenario: Deleting the notification deletes its task, in Todoist too
    Given I have a Todoist task, added in the year 2000, in the project Inbox
    And that Todoist task is active
    And the notification of that task is named "mine"
    And Todoist deletes that task
    When I set the status of "mine" to deleted
    Then the patched notification is deleted, was read just now, and is otherwise as "mine" was
    And that Todoist task is deleted

  Scenario: Snoozing the notification leaves its task alone
    Given I have a Todoist task, added in the year 2000, in the project Inbox
    And that Todoist task is active
    And the notification of that task is named "mine"
    And Todoist is not asked to change anything
    When I snooze "mine" until "2022-01-01T01:02:03Z"
    Then the patched notification is "mine" snoozed until "2022-01-01T01:02:03Z"
    And that Todoist task is active

  Scenario: Unsubscribing from a Todoist notification is not supported
    Given I have the notification "mine" of a Todoist item
    And Todoist is not asked to change anything
    When I try to unsubscribe from "mine"
    Then the app answers 400
    And the error message is "Unsupported action: Cannot update the status of Todoist notification <id>, update task's project", with <id> being the id of "mine"
    And "mine", read again, is exactly as it was

  Scenario: Another user cannot delete my notification: my task stays in Todoist, and their notification of the same item is left alone
    Given I have a Todoist task, added in the year 2000, in the project Inbox
    And the notification of that task is named "mine"
    And another user is signed in
    And that other user has their own Todoist connection, and their own notification "theirs" of the same Todoist item
    And Todoist is not asked anything more
    When that other user tries to change the status of "mine" to deleted
    Then the app answers 403
    And "mine", read again, is exactly as it was
    And the notification "theirs" of that other user, read by that other user, is exactly as it was
    And that Todoist task is active
