Feature: Changing and resetting a password
  A signed-in user changes their password by giving the current one. Changing
  or resetting a password signs out every other session of the account and
  emails the user. Guessing the current password is throttled like signing in.

  Background:
    Given the app lets users sign in with a password or a passkey

  Rule: A signed-in user can change their password

    Background:
      Given I have an account with a password
      And I signed in with my password

    @legacy:test_change_password::test_change_password
    Scenario: Changing the password keeps the user signed in and emails them
      When I change my password from "Very-harD-pasSword-5" to "New-very-harD-pasSword-5"
      Then the app answers 204
      And I am still signed in
      When I sign in from a new browser with the password "Very-harD-pasSword-5"
      Then the app answers 401
      When I sign in from a new browser with the password "New-very-harD-pasSword-5"
      Then the app answers 200
      And I received 1 password changed email

    @legacy:test_change_password::test_change_password_with_wrong_current_password
    Scenario: A wrong current password changes nothing
      When I change my password from "wrong-password" to "New-very-harD-pasSword-5"
      Then the app answers 400
      And the response text contains "The current password is incorrect"
      And I am still signed in
      When I sign in from a new browser with the password "Very-harD-pasSword-5"
      Then the app answers 200
      And I received 0 password changed emails

    @legacy:test_change_password::test_change_password_is_throttled_like_login
    Scenario: Guessing the current password locks the account like signing in does
      When I change my password from "wrong-password" to "New-very-harD-pasSword-5" 5 times, each time refused as invalid
      And I change my password from "Very-harD-pasSword-5" to "New-very-harD-pasSword-5"
      Then the app answers 429
      And the response has a "retry-after" header

    @legacy:test_change_password::test_change_password_rejects_invalid_new_password
    Scenario Outline: A new password that is <reason> is refused
      When I change my password from "Very-harD-pasSword-5" to "<new password>"
      Then the app answers 400
      When I sign in from a new browser with the password "Very-harD-pasSword-5"
      Then the app answers 200

      Examples:
        | reason              | new password                                                                                                                                                         |
        | too short           | short                                                                                                                                                                |
        | too long            | New-very-harD-pasSword-5New-very-harD-pasSword-5New-very-harD-pasSword-5New-very-harD-pasSword-5New-very-harD-pasSword-5New-very-harD-pasSword-5                    |
        | too common          | 1qaz2wsx3edc                                                                                                                                                         |
        | the current password | Very-harD-pasSword-5                                                                                                                                                 |

    @legacy:test_change_password::test_change_password_requires_front_origin
    Scenario: A request that does not say it comes from the app is refused
      When I change my password from "Very-harD-pasSword-5" to "New-very-harD-pasSword-5" in a request that does not say which website it comes from
      Then the app answers 400
      When I sign in from a new browser with the password "Very-harD-pasSword-5"
      Then the app answers 200

  Rule: Changing or resetting a password signs out the other sessions

    Background:
      Given I have an account with a password
      And I signed in with my password
      And I signed in again with my password, in another session

    @legacy:test_change_password::test_change_password_revokes_other_sessions
    Scenario: Changing the password signs out the user's other sessions
      Sessions are revoked with the precision of one second, so the change
      waits for the next second.

      When I wait for the next second
      And I change my password from "Very-harD-pasSword-5" to "New-very-harD-pasSword-5"
      Then the app answers 204
      And my second session is signed out
      And my first session is still signed in
      When I sign in from a new browser with the password "New-very-harD-pasSword-5"
      Then the app answers 200
      And that new browser is signed in

    @legacy:test_change_password::test_change_password_revokes_other_session_jwts_used_as_bearer
    Scenario: Changing the password stops the token of another session from being used as an API token
      When I wait for the next second
      And I change my password from "Very-harD-pasSword-5" to "New-very-harD-pasSword-5"
      Then the app answers 204
      And my session token is rejected as a bearer token

    @legacy:test_change_password::test_reset_password_revokes_sessions
    Scenario: Resetting the password signs out the user's sessions
      When I ask for my password to be reset
      Then the app answers 200
      When I wait for the next second
      And I reset my password to "New-very-harD-pasSword-5" with the link in the email
      Then the app answers 200
      And my first session is signed out

    @legacy:test_change_password::test_reset_password_revokes_session_jwts_used_as_bearer
    Scenario: Resetting the password stops the token of a session from being used as an API token
      When I ask for my password to be reset
      Then the app answers 200
      When I wait for the next second
      And I reset my password to "New-very-harD-pasSword-5" with the link in the email
      Then the app answers 200
      And my session token is rejected as a bearer token

  Rule: Only a user who has a password, and who is signed in, can change it

    @legacy:test_change_password::test_change_password_without_local_auth
    Scenario: A user who signs in with single sign-on has no password to change
      Given the app lets users sign in with single sign-on, a password and a passkey
      And I am signed in
      When I change my password from "Very-harD-pasSword-5" to "New-very-harD-pasSword-5"
      Then the app answers 400
      And I am still signed in

    @legacy:test_change_password::test_change_password_unauthenticated
    Scenario: A visitor who is not signed in cannot change a password
      When a visitor who is not signed in changes the password from "Very-harD-pasSword-5" to "New-very-harD-pasSword-5"
      Then the app answers 401
