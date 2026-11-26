Feature: Changing many notifications at once
  A user deletes or snoozes all the notifications of a source in one request.
  The side effects of the change, such as deleting them on GitHub, are done by
  background jobs that leave the queue empty. Creating a task from a
  notification needs a task title of a reasonable length.

  Background:
    Given I am signed in
    And I am connected to GitHub

  Scenario: A user deletes all their GitHub notifications at once
    Given GitHub accepts to delete the notification threads 1 and 2
    And I have a GitHub notification called "first"
    And I have a GitHub notification called "second"
    Then the notifications "first" and "second" are unread
    Given I note how many jobs were queued so far
    When I delete all my unread and read GitHub notifications
    Then 2 notifications were changed
    When I list my deleted notifications of GitHub
    Then I see exactly these notifications, in any order:
      | first  |
      | second |
    And that change queued 2 jobs, and all of them are done

  Scenario: A user snoozes all their GitHub notifications at once without changing their status
    Given I have a GitHub notification called "first"
    And I have a GitHub notification called "second"
    Then the notifications "first" and "second" are not snoozed
    Given I note how many jobs were queued so far
    When I snooze all my unread GitHub notifications until "2050-01-01T01:02:03Z"
    Then 2 notifications were changed
    And all the changed notifications are snoozed until "2050-01-01T01:02:03Z" and still unread
    When I list my snoozed unread notifications
    Then the snoozed notifications I see are exactly these, in any order:
      | first  |
      | second |
    And that change queued 2 jobs, and all of them are done

  Scenario: A task title that is too long is refused before the notification is looked up
    When I create a task from a notification that does not exist, with a title one character too long
    Then the app answers 400
