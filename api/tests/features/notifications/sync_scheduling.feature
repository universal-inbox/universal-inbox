Feature: Scheduling syncs
  Listing the notifications can ask for a sync of every connection of the user.
  Overlapping requests must not deadlock on the connections they schedule, and
  only a signed-in user can schedule a sync, of their own connections.

  Scenario: Concurrent requests that each schedule a sync never fail
    Given I am signed in
    And I am connected to GitHub
    And I am connected to Linear
    And I am connected to Todoist
    When I list my unread and read notifications 16 at a time, 5 times in a row, each time asking for a sync
    Then the app answered 200, 80 times

  Scenario: A visitor who is not signed in cannot schedule any sync
    When a visitor who is not signed in asks for a GitHub notification sync, a notification sync and a task sync
    Then the app answered 401, 3 times
