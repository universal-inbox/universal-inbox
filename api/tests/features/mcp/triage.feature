Feature: Triaging through MCP
  An AI client lists, acts on and turns into tasks the notifications of a user, through MCP tools.

  Background:
    Given I am signed in
    And I have an API token
    And I am connected to GitHub

  @legacy:test_mcp::scenario::manage_notifications_and_tasks_via_mcp
  Scenario: Notifications are listed, unsubscribed from and turned into a task, which is then completed
    Given I am connected to Todoist
    And Todoist can list my projects
    And I have a GitHub notification with the source id "123"
    And I have a GitHub notification with the source id "987"
    And I already read the GitHub notification "987"
    When I list my notifications through MCP, without syncing
    Then the MCP answer lists 2 notifications
    When I unsubscribe through MCP from every GitHub notification that I read
    Then that call changed 1 notification
    And that call unsubscribed the notification "987"
    Given Todoist creates a task for the notification "123"
    When I create through MCP a task from the notification "123", with the priority 4
    Then the task was created
    When I list my active tasks through MCP
    Then the MCP answer lists 1 task
    Given Todoist completes that task
    When I mark that task as done through MCP
    Then the MCP answer shows that task as done

  @legacy:test_mcp::scenario::acts_on_a_chosen_list_of_notifications
  Scenario: A mixed triage pass applies a different action to each notification and skips an unknown one
    Given I have a GitHub notification with the source id "1000"
    And I have a GitHub notification with the source id "1001"
    And I have a GitHub notification with the source id "1002"
    And I have a GitHub notification with the source id "1003"
    And GitHub accepts to mark the thread "1001" as done, once
    And GitHub accepts to mark the thread "1002" as done, once
    And GitHub accepts to ignore the thread "1002", once
    When I act through MCP on this list of notifications:
      | notification | action       | until                |
      | 1000         | mark_read    |                      |
      | 1001         | delete       |                      |
      | 1002         | unsubscribe  |                      |
      | 1003         | snooze_until | 2050-01-01T01:02:03Z |
      | unknown      | mark_read    |                      |
    Then that call changed 4 notifications
    And that call left the notification "1000" "Read"
    And that call left the notification "1001" "Deleted"
    And that call left the notification "1002" "Unsubscribed"
    And that call left the notification "1003" "Unread"
    And that call snoozed the notification "1003" until "2050-01-01T01:02:03Z"
    And that call did not touch the unknown notification
    And GitHub received the calls it was told to expect, and no job is left in the queue
