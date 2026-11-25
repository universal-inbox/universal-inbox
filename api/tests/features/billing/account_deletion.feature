Feature: Deleting the account of a paying user
  Deleting an account stops the Stripe billing first: a deleted user must never keep being charged.

  Background:
    Given the app lets users sign in with a password or a passkey
    And a user with a password exists

  @legacy:test_billing::account_deletion::deleting_a_paid_user_cancels_the_stripe_subscription
  Scenario: Deleting a paid user cancels their subscription, and unlinks the customer that Stripe keeps for the invoices
    Given Stripe reports at second 0 that the customer "cus_del1" checked out with the active subscription "sub_del1"
    And Stripe answers as a mock server, which lists no subscription of the customers
    And Stripe cancels the subscription "sub_del1" of the customer "cus_del1", once
    And Stripe accepts to unlink the customer "cus_del1" from the user
    When the user deletes their account
    Then the account is deleted
    And the subscription of the user is gone
    And Stripe was told to clear the user of the customer "cus_del1"
    And Stripe received every cancellation that it was told to expect, once

  @legacy:test_billing::account_deletion::deleting_a_user_also_cancels_subscriptions_only_stripe_knows
  Scenario: Deleting a paid user also cancels a subscription that only Stripe knows
    Given Stripe reports at second 0 that the customer "cus_del2" checked out with the active subscription "sub_del2"
    And Stripe answers as a mock server, which lists the active subscription "sub_del2_missed" of the customer "cus_del2"
    And Stripe cancels the subscription "sub_del2" of the customer "cus_del2", once
    And Stripe cancels the subscription "sub_del2_missed" of the customer "cus_del2", once
    And Stripe accepts to unlink the customer "cus_del2" from the user
    When the user deletes their account
    Then the account is deleted
    And Stripe received every cancellation that it was told to expect, once

  @legacy:test_billing::account_deletion::a_failed_stripe_cancellation_aborts_the_account_deletion
  Scenario: A cancellation that fails stops the deletion, so that the user can retry and the subscription is not orphaned
    Given Stripe reports at second 0 that the customer "cus_del3" checked out with the active subscription "sub_del3"
    And Stripe answers as a mock server, which lists no subscription of the customers
    And Stripe fails to cancel the subscription "sub_del3" of the customer "cus_del3", once, and still shows it as active
    When the user deletes their account
    Then the deletion fails, and the account still exists
    And the subscription of the user is still tracked as active, so that it cannot be orphaned
    And Stripe received every cancellation that it was told to expect, once

  @legacy:test_billing::account_deletion::an_already_canceled_subscription_does_not_block_the_account_deletion
  Scenario: A subscription that is already canceled does not block the deletion, nor does a failure to unlink the customer
    Given Stripe reports at second 0 that the customer "cus_del4" checked out with the active subscription "sub_del4"
    And Stripe answers as a mock server, which lists no subscription of the customers
    And Stripe refuses to cancel the subscription "sub_del4" of the customer "cus_del4", as it is already canceled
    And Stripe fails to unlink the customer "cus_del4" from the user
    When the user deletes their account
    Then the account is deleted
