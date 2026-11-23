Feature: Resetting a forgotten password
  A user who forgot their password asks for an email with a link. The link
  carries a token that can be used once, for a limited time, for the user it
  was sent to. The new password goes in the body of a request, never in the
  address.

  Background:
    Given the app lets users sign in with a password or a passkey
    And I have signed up with the email "john@doe.name" and a password

  @legacy:test_users::password_reset::test_send_password_reset_email
  Scenario: A user can ask for a password reset email
    When I ask for my password to be reset
    Then the app answers 200
    And 2 emails were sent in total
    And the latest email sent is the password reset email with my link

  @legacy:test_users::password_reset::test_reset_password
  Scenario: Following the emailed link resets the password
    The token must not be accepted in the address of the API request.

    When I ask for my password to be reset
    And I send the new password "New-very-harD-pasSword-5" to the API address that carries the reset token
    Then the app answers 404
    When I reset my password to "New-very-harD-pasSword-5" with the link in the email
    Then the app answers 200
    And my password reset token is used up
    When I sign in from a new browser with the password "New-very-harD-pasSword-5"
    Then the app answers 200
    And my latest email is a password changed email
    And my account records when the password reset was sent and done

  @legacy:test_users::password_reset::test_reset_password_unknown_user
  Scenario: A reset link with the id of another user is refused
    When I ask for my password to be reset
    And I reset my password to "New-very-harD-pasSword-5" with the link in the email but with the id of another user
    Then the app answers 400
    And the error message says the password reset token is invalid for the user in the link

  @legacy:test_users::password_reset::test_reset_password_expired_token
  Scenario: A reset link that is two hours old is refused and the old password still works
    When I ask for my password to be reset
    And two hours pass after my password reset email was sent
    And I reset my password to "New-very-harD-pasSword-5" with the link in the email
    Then the app answers 400
    When I sign in from a new browser with the password "New-very-harD-pasSword-5"
    Then the app answers 401

  @legacy:test_users::password_reset::test_reset_password_invalid_token
  Scenario: A reset link with a token that was never sent is refused
    When I ask for my password to be reset
    And I reset my password to "New-very-harD-pasSword-5" with a token I was never sent
    Then the app answers 400
    And the error message says the password reset token is invalid for the user in the link
