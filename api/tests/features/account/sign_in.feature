Feature: Signing in with a password
  A user signs in with their email address and password. A wrong password and
  an unknown address get the same answer. Signing out ends the session.

  Background:
    Given the app lets users sign in with a password or a passkey
    And I have signed up with the email "john@doe.name" and a password

  @legacy:test_users::login_user::test_login_user
  Scenario: A user signs in and gets a hardened session cookie
    Then I have no API tokens
    When I look at my account from a new browser
    Then the app answers 401
    And any session cookie it sets is cleared
    When I sign in from a new browser with the password "Very-harD-pasSword-5"
    Then the app answers 200
    And it sets a session cookie that is secure, HTTP-only, same-site lax, valid for the whole site and not tied to a domain
    And the response is my account
    And I have no API tokens
    And from that browser my account uses the email "john@doe.name" and is not verified yet

  @legacy:test_users::login_user::test_login_with_wrong_password
  Scenario: A wrong password is refused
    When I sign in from a new browser with the password "wrong"
    Then the app answers 401
    And the error message is "Unauthorized access: Invalid email address or password"

  @legacy:test_users::login_user::test_login_with_unknown_user
  Scenario: An email address nobody registered gets the same answer as a wrong password
    Given I have no account
    When I sign in from a new browser with the password ""
    Then the app answers 401
    And the error message is "Unauthorized access: Invalid email address or password"

  @legacy:test_users::login_user::test_logout_user
  Scenario: Signing out ends the session
    When I sign out
    Then any session cookie it sets is cleared
    And the app answers 200
    When I look at my account
    Then the app answers 401
    And any session cookie it sets is cleared
