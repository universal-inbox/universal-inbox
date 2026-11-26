Feature: Changing Linear notifications
  A change of a Linear notification reaches Linear first: deleting it archives it,
  unsubscribing from the notification of an issue also removes the user from the followers
  of the issue, and a snooze is recorded by Linear. When Linear refuses, the change is
  refused with a public error that tells nothing of Linear.

  Background:
    Given I am signed in
    And I am connected to Linear

  Rule: Deleting a notification archives it in Linear

    Scenario: Linear archives the notification that is deleted
      Given I have the Linear issue notification "mine"
      And Linear archives, once, the notification "mine"
      When I set the status of "mine" to deleted
      Then the patched notification is deleted, was read just now, and is otherwise as "mine" was

    Scenario: A Linear error is a public internal error and the notification stays
      Given I have the Linear issue notification "mine"
      And Linear answers, once, with the error "Entity not found" when asked to archive the notification "mine"
      When I try to delete "mine"
      Then the app answers 500
      And the answer is the public error "An internal error occurred. Please retry later or contact support with the correlation id."
      And "mine", read again, is exactly as it was

    Scenario: A Linear answer that says it did not archive is a public internal error
      Given I have the Linear issue notification "mine"
      And Linear answers, once, that it did not archive the notification "mine", without any error
      When I try to delete "mine"
      Then the app answers 500
      And the answer is the public error "An internal error occurred. Please retry later or contact support with the correlation id."
      And "mine", read again, is exactly as it was

  Rule: Unsubscribing from a notification

    Scenario: The user stops following the issue, and Linear archives the notification
      Given I have the Linear issue notification "mine"
      And Linear tells, once, that the user "user_id" and the user "other_user_id" follow the issue "issue_id" of the notification "mine"
      And Linear updates, once, the followers of the issue "issue_id" to the user "other_user_id" only
      And Linear archives, once, the notification "mine"
      When I set the status of "mine" to unsubscribed
      Then the patched notification is unsubscribed, was read just now, and is otherwise as "mine" was

    Scenario: The notification of a project has no followers to update, and Linear only archives it
      Given I have the Linear project notification "mine"
      And Linear tells, once, that the notification "mine" is about a project, which has no followers
      And Linear archives, once, the notification "mine"
      When I set the status of "mine" to unsubscribed
      Then the patched notification is unsubscribed, was read just now, and is otherwise as "mine" was

  Rule: Snoozing a notification

    Scenario: Linear records the snooze
      Given I have the Linear project notification "mine"
      And Linear records, once, the snooze of the notification "mine" until "2022-01-01T01:02:03Z"
      When I snooze "mine" until "2022-01-01T01:02:03Z"
      Then the patched notification is "mine" snoozed until "2022-01-01T01:02:03Z"

  Rule: Another user cannot act on my notification

    Scenario Outline: Another user cannot set my notification to <status>, and their notification of the same issue is left alone
      Given I have the Linear issue notification "mine"
      And another user is signed in
      And that other user is connected to Linear, syncing its tasks into the project Project2, due today
      And that other user has the Linear notification of that issue, "theirs", which they have read
      And Linear is not asked anything
      When that other user tries to change the status of "mine" to <status>
      Then the app answers 403
      And "mine", read again, is exactly as it was
      And the notification "theirs" of that other user, read by that other user, is exactly as it was

      Examples:
        | status       |
        | deleted      |
        | unsubscribed |
