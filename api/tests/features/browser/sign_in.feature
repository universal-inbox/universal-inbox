Feature: Signing in from the browser
  A registered user signs in with their email address and password to reach
  their notifications.

  Background:
    Given I have an account with sample notifications and integrations

  @legacy:test_user_login::test_user_can_login
  Scenario: A user signs in and browses their notifications and settings
    When I sign in from the login page
    Then I land on my notifications, away from the login page
    And I see at least 8 notifications
    And the first notification is not blank
    When I click the first notification
    Then a notification is selected
    When I open the settings from the sidebar
    Then I see at least 7 integration cards, among them "Github, Linear, Slack, Google Mail"

  @legacy:test_user_login::test_login_fails_with_wrong_password
  Scenario: Signing in with a wrong password shows an error
    When I open the login page
    And I sign in with the password "wrong_password"
    Then I see a sign-in error message
    And I am still on the login page

  @legacy:test_user_login::test_login_lockout_shows_friendly_error
  Scenario: Too many failed sign-ins lock the account with a friendly message
    After 5 failed attempts the account is locked (the API answers
    429 Too Many Requests).

    When I open the login page
    And I sign in with the password "definitely-wrong-pw" 6 times, seeing an error each time
    Then I see the sign-in error "Too many login attempts", not a raw technical error
