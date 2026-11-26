Feature: Verifying an email address
  A new account receives a verification email. Its link can be used once, for
  a limited time, and only for the user it was sent to.

  Background:
    Given the app lets users sign in with a password or a passkey
    And I have signed up with the email "john@doe.name" and a password

  Scenario: A user can ask for the verification email again
    Then 1 email was sent in total
    When I ask for the verification email to be sent again
    Then the app answers 200
    And my verification token is a new one
    And 2 emails were sent in total
    And the latest email sent is the verification email with my current link

  Scenario: Opening the link verifies the address, once
    When I open the link of the verification email
    Then the app answers 200
    And my email address is verified
    And my verification token is used up
    When I open the same link again
    Then the app answers 400

  Scenario: A link that is older than the validity period does not verify the address
    Given my verification email was sent just over the validity period ago
    When I open the link of the verification email
    Then the app answers 400
    And the response text contains "expired"
    And my email address is still not verified

  Scenario: A link with the id of another user is refused
    When I open the link of the verification email with another user's id
    Then the app answers 400
    And the error message says the verification token is invalid for the user in the link

  Scenario: A link with a token that was never sent is refused
    When I open a verification link with a token I was never sent
    Then the app answers 400
    And the error message says the verification token is invalid for the user in the link
