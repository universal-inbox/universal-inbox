Feature: Running the app without passwords
  An instance that only offers single sign-on answers every request that needs
  a password with a clear refusal, never with a server error.

  Background:
    Given the app only lets users sign in with single sign-on

  @legacy:test_users::local_auth_disabled::test_register_user
  Scenario: Nobody can register with a password
    When a visitor signs up with the email "john@doe.name" and the password "Very-harD-pasSword-5"
    Then the app refuses because local authentication is disabled

  @legacy:test_users::local_auth_disabled::test_login_user
  Scenario: Nobody can sign in with a password
    When a visitor signs in with the email "john@doe.name" and the password "Very-harD-pasSword-5"
    Then the app refuses because local authentication is disabled

  @legacy:test_users::local_auth_disabled::test_send_password_reset_email
  Scenario: Nobody can ask for a password reset email
    Given I have no account
    When I ask for my password to be reset
    Then the app refuses because local authentication is disabled

  @legacy:test_users::local_auth_disabled::test_reset_password
  Scenario: Nobody can reset a password
    Given I am signed in
    When I reset my password to "Very-harD-pasSword-5" with a token I was never sent
    Then the app refuses because local authentication is disabled

  @legacy:test_users::local_auth_disabled::test_add_local_auth_method
  Scenario: A signed-in user cannot add a password
    Given I am signed in
    When I add the password "Very-harD-pasSword-5" to my account
    Then the app refuses because local authentication is disabled

  @legacy:test_users::local_auth_disabled::test_change_password
  Scenario: A signed-in user cannot change a password
    Given I am signed in
    When I change my password from "Very-harD-pasSword-5" to "New-Very-harD-pasSword-5"
    Then the app refuses because local authentication is disabled

  @legacy:test_users::local_auth_disabled::test_reauthenticate_with_password
  Scenario: A signed-in user cannot confirm their identity with a password
    Given I am signed in
    When I confirm my identity with the password "Very-harD-pasSword-5"
    Then the app refuses because local authentication is disabled
