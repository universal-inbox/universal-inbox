Feature: Managing the ways to sign in
  A user can sign in with single sign-on, a password or a passkey, and can
  have several at once. They can list, add and remove them, but never remove
  the last one. Every change is announced by email.

  Rule: A user can list their sign-in methods

    @legacy:test_auth_methods::list_auth_methods::test_list_auth_methods_for_oidc_user
    Scenario: A user who signed in with single sign-on has that method
      Given I am signed in
      When I list my sign-in methods
      Then I have these sign-in methods:
        | single sign-on |

    @legacy:test_auth_methods::list_auth_methods::test_list_auth_methods_for_local_user
    Scenario: A user who signed up with a password has that method
      Given the app lets users sign in with a password or a passkey
      And I have signed up with the email "john@doe.name" and a password
      When I list my sign-in methods
      Then I have these sign-in methods:
        | password |

    @legacy:test_auth_methods::list_auth_methods::test_list_auth_methods_unauthenticated
    Scenario: A visitor who is not signed in cannot list sign-in methods
      Given the app lets users sign in with a password or a passkey
      When a visitor who is not signed in lists the sign-in methods of the app
      Then the app answers 401

  Rule: A user can add a password

    @legacy:test_auth_methods::add_local_auth_method::test_add_local_auth_to_oidc_user
    Scenario: A user who signed in with single sign-on adds a password
      Given the app lets users sign in with single sign-on, a password and a passkey
      And I am signed in
      When I list my sign-in methods
      Then I have these sign-in methods:
        | single sign-on |
      When I add the password "New-very-harD-pasSword-5" to my account
      Then the app answers 200
      And the added sign-in method is a password
      When I list my sign-in methods
      Then I have these sign-in methods:
        | password       |
        | single sign-on |
      And the database holds 2 sign-in methods for my account
      And an email tells me that "A password" was added as a sign-in method

    @legacy:test_auth_methods::add_local_auth_method::test_add_duplicate_local_auth
    Scenario: A user who already has a password cannot add another one
      Given the app lets users sign in with a password or a passkey
      And I have signed up with the email "john@doe.name" and a password
      When I add the password "Another-pasSword-5" to my account
      Then the app answers 409
      And the error message mentions "already"

  Rule: A user can add a passkey

    @legacy:test_auth_methods::add_passkey_auth_method::test_add_passkey_auth_to_local_user
    Scenario: A user who signed up with a password adds a passkey
      Given the app lets users sign in with a password or a passkey
      And I have signed up with the email "john@doe.name" and a password
      When I list my sign-in methods
      Then I have these sign-in methods:
        | password |
      When I add a passkey named "john_passkey" to my account
      Then the app answers 200
      And the added sign-in method is a passkey named "john_passkey"
      When I list my sign-in methods
      Then I have these sign-in methods:
        | password |
        | passkey  |
      And the database holds 2 sign-in methods for my account
      And an email tells me that "A passkey" was added as a sign-in method

    @legacy:test_auth_methods::add_passkey_auth_method::test_add_passkey_auth_to_oidc_user
    Scenario: A user who signed in with single sign-on adds a passkey
      Given the app lets users sign in with single sign-on, a password and a passkey
      And I am signed in
      When I list my sign-in methods
      Then I have these sign-in methods:
        | single sign-on |
      When I add a passkey named "john_passkey" to my account
      Then the app answers 200
      And the added sign-in method is a passkey named "john_passkey"
      When I list my sign-in methods
      Then I have these sign-in methods:
        | single sign-on |
        | passkey        |
      And the database holds 2 sign-in methods for my account

  Rule: A user can add single sign-on

    @legacy:test_auth_methods::add_oidc_auth_method::test_add_oidc_pkce_auth_to_local_user
    Scenario: A user who signed up with a password adds single sign-on
      Given the app lets users sign in with single sign-on, a password and a passkey
      And I have signed up with the email "john@doe.name" and a password
      When I list my sign-in methods
      Then I have these sign-in methods:
        | password |
      Given the identity provider knows the account "oidc-link-user-1234" as "John" "Doe" with the email "john@doe.name"
      When I link single sign-on to my account with an identity token for "john@doe.name"
      Then the app answers 200
      And the added sign-in method is a single sign-on
      When I list my sign-in methods
      Then I have these sign-in methods:
        | password       |
        | single sign-on |
      And the database holds 2 sign-in methods for my account
      And an email tells me that "Single sign-on" was added as a sign-in method

    @legacy:test_auth_methods::add_oidc_auth_method::test_add_duplicate_oidc_pkce_auth
    Scenario: A user who already has single sign-on cannot add it again
      Given the app lets users sign in with single sign-on, a password and a passkey
      And I am signed in
      When I link single sign-on to my account with an identity token for "test@example.com"
      Then the app answers 409

  Rule: A user can add a Google sign-in once

    @legacy:test_auth_methods::add_google_auth_method::test_add_google_auth_to_local_user
    Scenario: A user who signed up with a password links a Google account
      Linking goes through the app's service directly: the full redirect flow
      needs the identity provider to sign real RSA identity tokens.

      Given the app lets users sign in with single sign-on, a password and a passkey
      And I have signed up with the email "john@doe.name" and a password
      When I list my sign-in methods
      Then I have these sign-in methods:
        | password |
      When I link the Google account "google-user-12345" to my account
      Then the Google account is linked
      When I list my sign-in methods
      Then I have these sign-in methods:
        | password |
        | Google   |
      And the database holds 2 sign-in methods for my account

    @legacy:test_auth_methods::add_google_auth_method::test_add_duplicate_google_auth
    Scenario: A user who already linked a Google account cannot link another
      Given the app lets users sign in with single sign-on, a password and a passkey
      And I have signed up with the email "john@doe.name" and a password
      And my account is linked to the Google account "google-user-12345"
      When I link the Google account "google-user-67890" to my account
      Then linking a second Google account is refused

  Rule: A user can remove a sign-in method but never the last one

    @legacy:test_auth_methods::remove_auth_method::test_remove_auth_method_with_multiple_methods
    Scenario: A user with two sign-in methods removes one
      Given the app lets users sign in with single sign-on, a password and a passkey
      And I am signed in
      And I have added a password to my account
      When I list my sign-in methods
      Then I have these sign-in methods:
        | password       |
        | single sign-on |
      When I remove my password sign-in method
      Then the app answers 200
      When I list my sign-in methods
      Then I have these sign-in methods:
        | single sign-on |
      And the database holds 1 sign-in methods for my account
      And an email tells me that "A password" was removed as a sign-in method

    @legacy:test_auth_methods::remove_auth_method::test_cannot_remove_last_auth_method
    Scenario: A user cannot remove their only sign-in method
      Given I am signed in
      When I list my sign-in methods
      Then I have these sign-in methods:
        | single sign-on |
      When I remove my single sign-on sign-in method
      Then the app answers 400
      And the error message mentions "last"

  Rule: An app that does not offer passkeys refuses every passkey request

    No account, session or sign-in method can be created through a method the
    app does not offer.

    Background:
      Given the app lets users sign in with a password but not with a passkey

    @legacy:test_auth_methods::passkey_disabled::test_passkey_registration_is_forbidden
    Scenario: A visitor cannot register with a passkey
      When a visitor starts registering a passkey named "mallory_passkey"
      Then the app refuses because passkeys are disabled

    @legacy:test_auth_methods::passkey_disabled::test_passkey_authentication_start_is_forbidden
    Scenario: A visitor cannot start a passkey sign-in
      When a visitor starts signing in with the passkey of "mallory_passkey"
      Then the app refuses because passkeys are disabled

    @legacy:test_auth_methods::passkey_disabled::test_add_passkey_start_is_forbidden
    Scenario: A user cannot start adding a passkey
      Given I have signed up with a password
      When I start adding a passkey named "john_passkey" to my account
      Then the app refuses because passkeys are disabled
      And the database holds 1 sign-in methods for my account

    @legacy:test_auth_methods::passkey_disabled::test_passkey_reauthentication_start_is_forbidden
    Scenario: A user cannot confirm their identity with a passkey
      Given I have signed up with a password
      When I start confirming my identity with a passkey
      Then the app refuses because passkeys are disabled

    @legacy:test_auth_methods::passkey_disabled::test_passkey_finish_endpoints_are_forbidden
    Scenario: Finishing a passkey request is refused even with a genuine passkey
      The passkey comes from another app that does offer passkeys, so it is a
      well-formed credential.

      Given another app lets users sign in with a password or a passkey
      And a visitor registered a passkey named "alice" on the other app
      And I have signed up with a password
      When that visitor replays the passkey registration on this app
      Then the app refuses because passkeys are disabled
      And that browser is not signed in
      When that visitor replays a passkey sign-in started on the other app on this app
      Then the app refuses because passkeys are disabled
      When I replay that passkey registration to add it to my account on this app
      Then the app refuses because passkeys are disabled
      And the database holds 1 sign-in methods for my account
