Feature: Upgrading to the paid plan
  A user on the Free plan upgrades to the Paid plan through Stripe Checkout.
  Stripe is replaced by a fake that sends the browser back to the app.

  @legacy:test_billing_upgrade::test_user_can_upgrade_to_paid_plan
  Scenario: A new user upgrades from the Free plan to the Paid plan
    Given billing with Stripe is enabled
    When I register with a new email address
    And I verify my email address
    And I sign in from the login page
    And I open the billing page
    Then I see the "Free plan" with an "Upgrade to Paid" button
    When I click the "Upgrade to Paid" button
    Then I am sent to Stripe Checkout
    When Stripe reports that my checkout is completed
    And I open the billing page
    Then I see the "Paid plan" with a "Manage billing" button
    And there is no "Upgrade to Paid" button
