Feature: Signing in with a passkey
  Only the account's current passkey can sign in. A sign-in that started with a
  passkey that was since removed or replaced cannot be completed.

  Background:
    Given the app lets users sign in with a password or a passkey
    And I have signed up with a password
    And I have registered a passkey

  @legacy:test_passkey_login::test_a_still_registered_passkey_logs_in
  Scenario: A passkey that is still registered signs in
    When I start a passkey sign-in from a signed-out browser
    And I complete the passkey sign-in with my first device
    Then the app answers 200
    And that browser is signed in as me

  @legacy:test_passkey_login::test_a_replaced_passkey_cannot_finish_a_login_started_before_the_replacement
  Scenario: A replaced passkey cannot finish a sign-in started before the replacement
    When I start a passkey sign-in from a signed-out browser
    And I remove my passkey
    Then the app answers 200
    When I register another passkey on a new device
    And I complete the passkey sign-in with my first device
    Then the app answers 401
    And that browser is not signed in

  @legacy:test_passkey_login::test_a_removed_passkey_cannot_finish_a_pending_login
  Scenario: A removed passkey cannot finish a pending sign-in
    When I start a passkey sign-in from a signed-out browser
    And I remove my passkey
    Then the app answers 200
    When I complete the passkey sign-in with my first device
    Then the app answers a client error
    And that browser is not signed in
