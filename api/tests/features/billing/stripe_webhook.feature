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
    Then my billing state says the Free plan, with the subscription status canceled
    When I send a POST request to "/api/billing/stripe/webhook"
    Then the app answers 400
    And the answer is the refusal "Missing Stripe-Signature header"

  Scenario: An event that Stripe did not sign, or signed at the wrong time, is refused, changes nothing and does not use up its event id
    Given Stripe is not asked anything
    And I remember the subscription of the other user
    When the webhook of Stripe receives, without a signature, the event "evt_upgrade" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to active
    Then the app answers 400
    And the answer is the refusal "Missing Stripe-Signature header"
    When the webhook of Stripe receives, signed with a wrong secret, the event "evt_upgrade" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to active
    Then the app answers 400
    And the answer is the refusal "Invalid Stripe webhook signature"
    When the webhook of Stripe receives, signed correctly 10 minutes ago, the event "evt_upgrade" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to active
    Then the app answers 400
    And the answer is the refusal "Invalid Stripe webhook signature"
    When the webhook of Stripe receives, signed correctly 10 minutes ahead, the event "evt_upgrade" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to active
    Then the app answers 400
    And the answer is the refusal "Invalid Stripe webhook signature"
    And the subscription of the user is canceled
    When I read my billing state
    Then my billing state says the Free plan, with the subscription status canceled
    When the webhook of Stripe receives, signed correctly 4 minutes ago, the event "evt_upgrade" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to active
    Then the app answers 204
    And the subscription of the user is active
    When I read my billing state
    Then my billing state says the Paid plan, with the subscription status active
    And the subscription of the other user is exactly as I remembered it

  Scenario: An event that Stripe delivers twice is applied once, whatever the second delivery says
    Given Stripe is not asked anything
    When the webhook of Stripe receives, signed correctly, the event "evt_upgrade" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to active
    Then the app answers 204
    And the subscription of the user is active
    When the webhook of Stripe receives, signed correctly, the event "evt_upgrade" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to canceled
    Then the app answers 200
    And the subscription of the user is active
    When the webhook of Stripe receives, signed correctly, the event "evt_downgrade" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to canceled
    Then the app answers 204
    And the subscription of the user is canceled

  Scenario: A forged cancellation of the subscription of another user is refused, and a real event of mine does not touch theirs
    Given Stripe is not asked anything
    And I remember the subscription of the other user
    When the webhook of Stripe receives, signed with a wrong secret, the event "evt_forged" telling that the subscription "sub_theirs" of the customer "cus_theirs" was updated to canceled
    Then the app answers 400
    And the answer is the refusal "Invalid Stripe webhook signature"
    And the subscription of the other user is exactly as I remembered it
    When the webhook of Stripe receives, signed correctly, the event "evt_late" telling that the subscription "sub_mine" of the customer "cus_mine" was updated to past due
    Then the app answers 204
    And the subscription of the user is past due
    And the subscription of the other user is exactly as I remembered it
