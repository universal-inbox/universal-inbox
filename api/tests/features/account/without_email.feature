Feature: Running the app without email
  An instance that cannot send emails still works: addresses count as verified
  right away, an address change applies immediately, and the features that
  need an email are not implemented.

  Rule: The app tells the web app whether it can send emails

    Scenario: An app that can send emails says so
      Given the app lets users sign in with a password or a passkey
      When I ask how the app is configured
      Then the app reports that email is enabled

    Scenario: An app that cannot send emails says so
      Given the app lets users sign in with a password or a passkey
      And the app does not send emails
      When I ask how the app is configured
      Then the app reports that email is disabled

  Rule: Without email, nothing waits for an email

    Background:
      Given the app lets users sign in with a password or a passkey
      And the app does not send emails

    Scenario: A new address is verified as soon as the user signs up
      When I sign up with the email "john@doe.name" and the password "Very-harD-pasSword-5"
      Then my email address is verified straight away, without any email
      And 0 emails were sent in total

    Scenario: A change of address applies immediately
      Given I have signed up with the email "john@doe.name" and a password
      When I request the change of my email address to "jane@doe.name"
      Then the app answers 200
      And the response is my account with the verified email "jane@doe.name"
      And no email change is pending
      And my account, as the app now shows it, uses the email "jane@doe.name"
      And 0 emails were sent in total

    Scenario: An address that belongs to someone else is refused
      Given another user signed up with the email "jane@doe.name"
      And I have signed up with the email "john@doe.name" and a password
      When I request the change of my email address to "jane@doe.name"
      Then the app answers 400
      And my account, as the app now shows it, uses the email "john@doe.name"

    Scenario: A verification email cannot be sent again
      Given I have signed up with the email "john@doe.name" and a password
      When I ask for the verification email to be sent again
      Then the app answers 501
      And the error message is "Email is not configured on this instance"

    Scenario: A password cannot be reset
      Given I have signed up with the email "john@doe.name" and a password
      When I ask for my password to be reset
      Then the app answers 501
      And no password reset token exists for my account
