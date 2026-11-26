Feature: Registering and signing in with a passkey, step by step
  A visitor registers a passkey and later signs in with it in two steps, each
  bound to the browser that started it. The app answers the same way whether or
  not a username is registered, refuses a finish that does not match its start,
  and never lets the state of one flow complete another.

  Background:
    Given the app lets users sign in with a password or a passkey

  Rule: Starting a ceremony does not reveal which usernames are registered

    Scenario: Starting a registration for a registered username looks like starting it for a free one
      Given a visitor registered a passkey for the username "alice_existing_passkey"
      When a visitor starts registering a passkey for the username "bob_fresh_passkey"
      And a visitor starts registering a passkey for the username "alice_existing_passkey"
      Then the app answered 200 every time
      And both answers are registration challenges for the same relying party
      And the first challenge is for the user "bob_fresh_passkey" and the second one for the user "alice_existing_passkey"
      And the second answer does not say that the username already exists

    Scenario: Starting a sign-in for an unknown username looks like starting it for a registered one
      Given a visitor registered a passkey for the username "carol_registered_passkey"
      When a visitor starts a passkey sign-in as "carol_registered_passkey"
      And a visitor starts a passkey sign-in as "ghost_not_registered_passkey"
      Then the app answered 200 every time
      And both answers are sign-in challenges for the same relying party
      And neither sign-in challenge lists a credential the sign-in may use
      And the first answer does not contain the ID of the registered credential
      And the second answer does not repeat the username "ghost_not_registered_passkey" nor say that an item was not found
      And both answers have the same structure once their challenge and nonce are left out

  Rule: A sign-in in progress belongs to the browser that started it

    Scenario: Someone else starting a sign-in for the same username does not break it
      Given a visitor registered a passkey for the username "dave_passkey_victim"
      When a visitor starts a passkey sign-in as "dave_passkey_victim"
      And another visitor starts a passkey sign-in as "dave_passkey_victim"
      Then the app answered 200 every time
      When the device of "dave_passkey_victim" completes the first sign-in that was started
      Then the app answers 200

  Rule: A registration is finished only as it was started

    Scenario: Finishing a registration for a username that is taken is refused without details
      Given a visitor registered a passkey for the username "alice_taken_username"
      When a visitor starts registering a passkey for the username "alice_taken_username"
      Then the app answers 200
      When the device answers the registration challenge
      And the passkey registration is finished as a new account
      Then the app answers 409
      And the error message is "This username is already taken. Please choose a different one."
      And the response mentions neither the database, the username nor any identifier

    Scenario: A registration finished with a nonce the app did not issue is refused
      When a visitor starts registering a passkey for the username "nonce_match_alice"
      Then the app issued a nonce for the registration
      When the device answers the registration challenge
      And the registration is finished with a nonce the app did not issue
      Then the app answers 401
      When a visitor starts registering a passkey for the username "nonce_match_alice_v2"
      And the device answers the registration challenge
      And the passkey registration is finished as a new account
      Then the app answers 200

    Scenario: A registration finished from another website is refused, from the app's own website it succeeds
      When a visitor starts registering a passkey for the username "origin_check_bob"
      And the device answers the registration challenge
      And the registration is finished from the website "https://evil.example"
      Then the app answers 400
      When the passkey registration is finished as a new account
      Then the app answers 200

    Scenario: A registration started to add a passkey cannot be finished as a new account
      Given I have signed up with the email "carol@doe.name" and a password
      When I start adding a passkey named "carol_passkey"
      And the device answers the registration challenge
      And the passkey registration is finished as a new account
      Then the app does not answer with a success
      When the passkey registration is finished as an added passkey
      Then the app answers 200
