Feature: Deleting the account
  A signed-in user deletes their account by typing their email address. The
  deletion removes everything the account owns, including stored credentials,
  and ends the session.

  Scenario: A user deletes their account and everything it owns
    Given I am signed in
    And I have an API token
    And I am connected to GitHub
    And I have a GitHub notification
    And a Slack bridge action is waiting for that notification
    Then my account owns data in the tables of audit events, API tokens, integration connections, notifications, Slack bridge actions and third-party items
    When I delete my account, confirming with my email address
    Then the app answers 200
    And the app says the deletion succeeded
    And no data of my account is left, nor any stored credential
    And my session no longer works

  Scenario Outline: Deleting needs the user to confirm with their own email address
    Given I am signed in
    When I delete my account, confirming with "<confirmation>"
    Then the app answers 400
    And my account is still there

    Examples:
      | confirmation              |
      |                           |
      | someone.else@example.com  |
      | DELETE                    |

  Scenario: A visitor who is not signed in cannot delete an account
    When a visitor who is not signed in deletes an account, confirming with "john@doe.name"
    Then the app answers 401
