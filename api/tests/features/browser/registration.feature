Feature: Registering from the browser
  A new user registers with their email address and a password. They must
  verify their email address before they can sign in.

  Scenario: A new user registers, verifies their email, signs in and opens the main pages
    When I register with a new email address
    Then I am asked to confirm my email address
    When I verify my email address
    And I sign in from the login page
    Then I can open the tasks page from the navigation
    And I can open the settings page from the navigation
