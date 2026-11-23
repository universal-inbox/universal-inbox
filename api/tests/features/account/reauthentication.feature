Feature: Confirming your identity before sensitive changes
  Changing the email address, the sign-in methods or the API tokens needs a
  sign-in or a confirmation of identity that is recent. Changing a name does
  not. An API token can never run these operations, however recent its creation.

  Rule: A browser session must be recent to run a sensitive operation

    Background:
      Given the app lets users sign in with a password or a passkey
      And a sign-in only counts as recent for a short time
      And I have signed up with a password

    @legacy:test_reauthentication::test_sensitive_operations_need_a_recent_authentication
    Scenario: Once the sign-in is no longer recent, sensitive operations ask for a new confirmation
      When I wait until my sign-in is no longer recent
      And I ask to change my email address
      Then the app asks me to sign in again
      When I ask to add a password
      Then the app asks me to sign in again
      When I ask to start adding a passkey
      Then the app asks me to sign in again
      When I ask to remove my password
      Then the app asks me to sign in again
      When I ask to create an API token
      Then the app asks me to sign in again
      When I ask to link single sign-on
      Then the app asks me to sign in again

    @legacy:test_reauthentication::test_a_name_change_does_not_need_a_recent_authentication
    Scenario: Changing a name does not need a recent sign-in
      When I wait until my sign-in is no longer recent
      And I change my first name to "John" and keep my email address
      Then the app answers 200

    @legacy:test_reauthentication::test_password_reauthentication_unlocks_sensitive_operations
    Scenario: Confirming with the password unlocks sensitive operations again
      When I wait until my sign-in is no longer recent
      And I confirm my identity with the password "Wrong-pasSword-5"
      Then the app answers 400
      When I ask to create an API token
      Then the app asks me to sign in again
      When I confirm my identity with the password "Very-harD-pasSword-5"
      Then the app answers 204
      When I ask to create an API token
      Then the app answers 200

    @legacy:test_reauthentication::test_a_wrong_reauthentication_password_does_not_log_the_user_out
    Scenario: A wrong password at confirmation does not sign the user out
      When I confirm my identity with the password "Wrong-pasSword-5"
      Then the app answers 400
      And I am still signed in

    @legacy:test_reauthentication::test_passkey_reauthentication_unlocks_sensitive_operations
    Scenario: Confirming with a passkey unlocks sensitive operations again
      Given I have registered a passkey
      When I wait until my sign-in is no longer recent
      And I ask to create an API token
      Then the app asks me to sign in again
      When I start confirming my identity with a passkey
      Then the app answers 200
      When I finish confirming my identity with my first device
      Then the app answers 204
      When I ask to create an API token
      Then the app answers 200

    @legacy:test_reauthentication::test_a_replaced_passkey_cannot_reauthenticate
    Scenario: A passkey that was replaced cannot confirm an identity
      Given I have registered a passkey
      When I start confirming my identity with a passkey
      Then the app answers 200
      When I remove my passkey
      Then the app answers 200
      When I register another passkey on a new device
      And I wait until my sign-in is no longer recent
      And I finish confirming my identity with my first device
      Then the app answers a client error
      When I ask to create an API token
      Then the app asks me to sign in again

  Rule: A user who signs in with single sign-on confirms with the identity provider

    @legacy:test_reauthentication::test_google_reauthentication_redirects_to_google
    Scenario: Confirming an identity sends the user to the identity provider to pick an account
      Given the app lets users sign in with single sign-on, a password or a passkey, and refuses the domain blocked.com
      And I am signed in
      When I ask to confirm my identity with the identity provider
      Then the app answers 200
      And the identity provider's sign-in address asks me to select an account

  Rule: An API token can never run a sensitive operation

    Background:
      Given the app lets users sign in with a password or a passkey
      And a sign-in only counts as recent for a short time
      And I have signed up with a password

    @legacy:test_reauthentication::test_an_api_token_cannot_run_sensitive_operations
    Scenario: A token created a moment ago still cannot create tokens, change the email or remove a method
      Given I have an API token
      When I use my API token to create an API token
      Then the app asks me to sign in again
      When I use my API token to change my email address
      Then the app asks me to sign in again
      When I use my API token to remove my password
      Then the app asks me to sign in again
