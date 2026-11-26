Feature: The webhook of Stripe
  With a billing configuration, the billing routes exist, and Stripe tells the app what
  happens to a subscription through a webhook. The app only believes an event that carries
  a recent signature made with the secret it shares with Stripe: anyone can reach the
  webhook, and an event that says "active" gives the Paid plan.

  Background:
    Given billing with Stripe is enabled
    And the app lets users sign in with single sign-on, a password and a passkey
    And I am signed in
    And my subscription is the one that the events of Stripe are about
    And Stripe reports at second 0 that the customer "cus_mine" checked out with the active subscription "sub_mine"
    And Stripe reports at second 200 that the subscription "sub_mine" of the customer "cus_mine" was deleted
    And another user with a password pays, as the customer "cus_theirs" with the active subscription "sub_theirs"

  Scenario: With a billing configuration, the billing routes exist
    When I read my billing state
    Then my billing state says the free plan, with the subscription status canceled
    When I send a POST request to "/api/billing/stripe/webhook"
    Then the app answers 400
    And the answer is the refusal "Missing Stripe-Signature header"

  Scenario: An event that Stripe did not sign, or signed too long ago, is refused and changes nothing
    Given Stripe is not asked anything
    When the webhook of Stripe receives, without a signature, the event "evt_forged_1" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to active
    Then the app answers 400
    And the answer is the refusal "Missing Stripe-Signature header"
    When the webhook of Stripe receives, signed with a wrong secret, the event "evt_forged_2" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to active
    Then the app answers 400
    And the answer is the refusal "Invalid Stripe webhook signature"
    When the webhook of Stripe receives, signed correctly 10 minutes ago, the event "evt_forged_3" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to active
    Then the app answers 400
    And the answer is the refusal "Invalid Stripe webhook signature"
    And the subscription of the user is canceled
    When I read my billing state
    Then my billing state says the free plan, with the subscription status canceled
    When the webhook of Stripe receives, signed correctly, the event "evt_real" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to active
    Then the app answers 204
    And the subscription of the user is active
    When I read my billing state
    Then my billing state says the paid plan, with the subscription status active
    And the subscription of the other user is active

  Scenario: A forged cancellation of the subscription of another user is refused, and a real one of mine does not touch theirs
    Given Stripe is not asked anything
    When the webhook of Stripe receives, signed with a wrong secret, the event "evt_forged_4" telling that the subscription "sub_theirs" of the customer "cus_theirs" was updated to canceled
    Then the app answers 400
    And the answer is the refusal "Invalid Stripe webhook signature"
    And the subscription of the other user is active
    When the webhook of Stripe receives, signed correctly, the event "evt_real_2" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to past due
    Then the app answers 204
    And the subscription of the user is past due
    And the subscription of the other user is active
