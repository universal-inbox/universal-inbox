Feature: Signing in with the identity provider
  Users sign in with the company's identity provider. The app trusts it once it
  confirms the user's tokens, and it keeps the latest identity token of each
  user.

  Rule: A session starts when the identity provider confirms the tokens

    @legacy:test_auth::authenticate_session::test_authenticate_session_creation
    Scenario: Signing in creates the account and keeps the latest identity token
      Given the identity provider knows me as "John" "Doe" with the email "test@example.com"
      When I sign in with the first identity token
      Then the app answers 200
      And my account has the first name "John" and the last name "Doe"
      And my account stores the first identity token
      And no API access token exists for my account
      When I sign in with the second identity token
      Then the app answers 200
      And my account stores the second identity token

    @legacy:test_auth::authenticate_session::test_authenticate_session_creation_wrong_access_token
    Scenario: Signing in with an access token the identity provider rejects is refused
      Given the identity provider rejects my access token
      When I sign in with the first identity token
      Then the app answers 401

  Rule: Every sign-in starts a fresh flow

    @legacy:test_auth::authorize_session::test_authorize_session_clears_cached_authorization_url
    Scenario: Asking to sign in twice gives two different addresses with different states
      Starting a second sign-in must not reuse the first one's anti-forgery
      state. A flow that was reused would make the callback sign the user in
      instead of linking their account (universal-inbox-bkj.15).

      Given the identity provider is available
      When I ask to sign in with the identity provider
      And I ask to sign in with the identity provider
      Then the app answered 200 every time
      And the second address differs from the first
      And the two addresses carry a different "state"
