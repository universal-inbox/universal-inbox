Feature: Slack bridge actions
  The Slack extension replays the actions of the bridge in the browser of the user,
  and reports each one as completed or failed. A completed action stays completed, a
  permanently failed action stays failed, an action that failed five times is
  permanently failed, and neither the deletion of the user nor the one of a
  notification is blocked by the actions that refer to them.

  Background:
    Given I am signed in

  Rule: The extension reports an action

    Scenario: A pending action is completed
      Given I have a Pending Slack bridge action
      When the Slack extension reports that action as completed
      Then the app answers 200
      And that action is Completed
      And that action has a completion date

    Scenario: A failed action is completed
      Given I have a Failed Slack bridge action
      When the Slack extension reports that action as completed
      Then the app answers 200
      And that action is Completed
      And that action has a completion date

    Scenario: A stale failure report does not revert a completed action
      Given I have a Pending Slack bridge action
      And the Slack extension reports that action as completed
      And I remember the state of that action
      When the Slack extension reports that action as failed, with the error "stale error"
      Then the app answers 200
      And that action is exactly as I remembered it
      And that action is Completed
      And that action was retried 0 times

    Scenario: A completion report does not revive a permanently failed action
      Given I have a PermanentlyFailed Slack bridge action
      When the Slack extension reports that action as completed
      Then the app answers 200
      And that action is PermanentlyFailed
      And that action has no completion date

    Scenario: An action that failed five times is permanently failed
      Given I have a Pending Slack bridge action
      When the Slack extension reports that action as failed 4 times, with the error "transient"
      Then that action is Failed
      And that action was retried 4 times
      When the Slack extension reports that action as failed, with the error "terminal"
      Then that action is PermanentlyFailed
      And that action was retried 5 times
      And that action keeps the failure message "terminal"

  Rule: The actions do not block a deletion

    Scenario: A user with pending actions can be deleted, and their actions go with them
      Given I have a Pending Slack bridge action
      When my user is deleted
      Then that user is deleted, with that action

    Scenario: A notification that a pending action refers to can be deleted, and the action is kept
      Given I am connected to GitHub
      And I have a GitHub notification
      And I have a Pending Slack bridge action about that notification
      When that notification is deleted from the database
      Then that action is kept, without any reference to that notification
      And that action is Pending

  Rule: An action is reported by its owner only

    Scenario: The extension of another user cannot complete my action
      Given I have a Pending Slack bridge action
      And another user is signed in
      And I remember the state of that action
      When the Slack extension of another user reports that action as completed
      Then the app answers 200
      And that action is exactly as I remembered it
      And that action is Pending

    Scenario: The extension of another user cannot fail my action
      Given I have a Pending Slack bridge action
      And another user is signed in
      And I remember the state of that action
      When the Slack extension of another user reports that action as failed, with the error "not mine"
      Then the app answers 200
      And that action is exactly as I remembered it
      And that action is Pending
      And that action was retried 0 times
