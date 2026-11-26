Feature: Protecting accounts from guessing and abuse
  Wrong passwords lock an account, and the emails an account can trigger
  (sign-in attempts, registrations, password resets, verification emails) are
  limited per email address, whatever network address the requests come from.
  An unknown email address is treated exactly like a registered one, so nobody
  can learn which addresses have an account.

  Rule: Too many wrong passwords lock the account

    Background:
      Given the app lets users sign in with a password or a passkey

    Scenario: The account locks after 5 wrong passwords
      The 5th wrong password answers like the others but locks the account.
      The next attempt is refused before the password is even checked.

      Given I have an account with a password
      When I sign in 5 times with the password "wrong-password", each time answered 401
      And I sign in from a new browser with the password "wrong-password"
      Then the app answers 429
      And the response has a "retry-after" header
      When I sign in from a new browser with the password "Very-harD-pasSword-5"
      Then the app answers 429
      And I received 1 account lockout email

    Scenario: A successful sign-in clears the wrong passwords counted so far
      Given I have an account with a password
      When I sign in 4 times with the password "wrong-password", each time answered 401
      And I sign in from a new browser with the password "Very-harD-pasSword-5"
      Then the app answers 200
      When I sign in 4 times with the password "wrong-password", each time answered 401
      Then I received 0 account lockout emails

    Scenario: An email address nobody registered is throttled like a registered one
      The answers are the same, but no lockout email is sent because no
      account exists. Nothing leaks.

      Given I have no account
      When I sign in 5 times with the password "wrong-password", each time answered 401
      And I sign in from a new browser with the password "wrong-password"
      Then the app answers 429
      And the response has a "retry-after" header
      And I received 0 account lockout emails

  Rule: Each email address has its own budget, whatever address the requests come from

    Background:
      Given each account may make 8 login requests and each email address may receive 3 account emails in a period
      And the app lets users sign in with a password or a passkey

    Scenario: Password resets are limited per email address
      Given I have an account with a password
      And an email address that no account uses
      When I request 4 password resets for each of the registered and the unregistered email addresses, each from a different network address
      Then for both email addresses the first 3 requests were answered 200 and the next one was throttled with a Retry-After header
      And both email addresses got exactly the same responses
      And the registered email address received 3 emails

    Scenario: Registrations are limited per email address
      The first registration of a new address creates the account. Later ones
      find it registered, which emails its owner as well.

      Given I have an account with a password
      And an email address that no account uses
      When I request 4 registrations for each of the registered and the unregistered email addresses, each from a different network address
      Then for both email addresses the first 3 requests were answered 200 and the next one was throttled with a Retry-After header
      And both email addresses got exactly the same responses
      And the registered email address received 3 emails

    Scenario: Registrations and password resets share the same budget
      Given I have an account with a password
      When I alternate 3 times between registering the registered email address and asking for its password to be reset, each from a different network address and each answered 200
      And I ask for a password reset for the registered email address from a new network address
      Then the app answers 429
      When I register the registered email address again from a new network address
      Then the app answers 429

    Scenario: Sign-in requests are limited per account, successful ones included
      Every attempt counts, not only the wrong passwords the lockout tracks.

      Given I have an account with a password
      When I sign in with my password 8 times, each from a different network address and each answered 200
      And I sign in with my password from a new network address
      Then the app answers 429
      And the response has a "retry-after" header

    Scenario: Verification emails are limited per email address
      Given I have an account with a password
      And I signed in with my password
      When I ask 3 times for the verification email to be sent again, each from a different network address and each answered 200
      And I ask for the verification email to be sent again from a new network address
      Then the app answers 429
      And the response has a "retry-after" header
      And the registered email address received 3 emails

    Scenario: Verification emails and password resets share the same budget
      Given I have an account with a password
      And I signed in with my password
      When I alternate 3 times between asking for the verification email again and asking for a password reset, each from a different network address and each answered 200
      And I ask for a password reset for the registered email address from a new network address
      Then the app answers 429
      When I ask for the verification email to be sent again from a new network address
      Then the app answers 429
