Feature: Signing out
  Signing out ends the session for good, even for someone who copied its
  cookie. Other sessions of the same user are not affected.

  Rule: Signing out of the identity provider

    @legacy:test_auth::close_session::test_close_session
    Scenario: Signing out clears the session cookie and sends the user to the identity provider
      Given I am signed in
      When I sign out
      Then the app answers 200
      And the session cookie is cleared
      And I am sent to the identity provider's logout page for my session

  Rule: A signed-out session cannot be used again

    Background:
      Given the app lets users sign in with a password or a passkey
      And I have an account with a password

    @legacy:test_auth::close_session::revocation::test_logged_out_session_cookie_is_rejected
    Scenario: A copy of the session cookie stops working once the user signs out
      Given I signed in with my password
      Then a copy of my session cookie is accepted
      When I sign out
      Then the app answers 200
      And a copy of my session cookie is rejected
      And my first session is signed out

    @legacy:test_auth::close_session::revocation::test_session_jwt_is_rejected_as_bearer_token
    Scenario: The token inside the session cookie cannot be used as an API token
      Given I signed in with my password
      Then a copy of my session cookie is accepted
      And my session token is rejected as a bearer token

    @legacy:test_auth::close_session::revocation::test_logged_out_session_jwt_is_rejected_as_bearer_token
    Scenario: The token inside a signed-out session cookie cannot be used as an API token
      Given I signed in with my password
      When I sign out
      Then the app answers 200
      And my session token is rejected as a bearer token

    @legacy:test_auth::close_session::revocation::test_logout_keeps_the_other_sessions_of_the_user
    Scenario: Signing out of one session keeps the user's other sessions
      Given I signed in with my password
      And I signed in again with my password, in another session
      When I sign out of my first session
      Then the app answers 200
      And my second session is still signed in
