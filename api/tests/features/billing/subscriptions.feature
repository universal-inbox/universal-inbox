Feature: Subscriptions and Stripe events
  A user is on the Free plan until Stripe says that they pay. The app keeps one subscription per user,
  from the events that Stripe sends, which arrive late, twice or in the wrong order, and from a nightly reconciliation.

  Scenario: Without a billing configuration, the billing routes do not exist
    When I request "/api/billing/me"
    Then the app answers 404
    When I send a POST request to "/api/billing/checkout-session"
    Then the app answers 404
    When I send a POST request to "/api/billing/portal-session"
    Then the app answers 404
    When I send a POST request to "/api/billing/stripe/webhook" with the headers:
      | Stripe-Signature | t=1,v1=deadbeef |
    Then the app answers 404

  Scenario: The Free plan syncs at most once a day, while the Paid plan follows the global minimum
    Given the app lets users sign in with a password or a passkey
    When I look up the sync interval of the Free plan for notifications, with a global minimum of 60 minutes
    Then that interval is 1440 minutes
    When I look up the sync interval of the Free plan for notifications, with a global minimum of 2880 minutes
    Then that interval is 2880 minutes
    When I look up the sync interval of the Paid plan for tasks, with a global minimum of 5 minutes
    Then that interval is 5 minutes

  Rule: The events of Stripe apply in the order that they happened

    Background:
      Given the app lets users sign in with a password or a passkey
      And a user with a password exists

    Scenario: A checkout of a customer that no row knows is linked to the user named in its metadata
      Then the user has no subscription
      When Stripe reports at second 0 that the customer "cus_b1" checked out with the active subscription "sub_b1"
      Then the subscription is linked to the customer "cus_b1" and the subscription "sub_b1"
      And the user is on the Paid plan

    Scenario: An older update that arrives after the cancellation does not bring the subscription back
      Given Stripe reports at second 0 that the customer "cus_b2" checked out with the active subscription "sub_b2"
      When Stripe reports at second 200 that the subscription "sub_b2" of the customer "cus_b2" was deleted
      Then the subscription of the user is canceled
      When Stripe reports at second 100 that the subscription "sub_b2" of the customer "cus_b2" was updated to active
      Then the subscription of the user is canceled
      And the user is on the Free plan
      When Stripe reports at second 300 that the subscription "sub_b2" of the customer "cus_b2" was updated to active
      Then the subscription of the user is active

    Scenario: A stale write that raced with the cancellation cannot overwrite it, but a newer one can
      Given Stripe reports at second 0 that the customer "cus_b4" checked out with the active subscription "sub_b4"
      And Stripe reports at second 200 that the subscription "sub_b4" of the customer "cus_b4" was deleted
      When a write from second 100 that says active and Paid is attempted on the subscription
      Then the write reports the subscription as canceled
      And the subscription of the user is canceled
      And the user is on the Free plan
      And the watermark of the ordering guard did not move back
      When a write from second 300 that says active and Paid is attempted on the subscription
      Then the subscription of the user is active

    Scenario: A paid invoice uses the subscription that was fetched beforehand, without asking Stripe again
      Given Stripe reports at second 0 that the customer "cus_b3" checked out with the past due subscription "sub_b3"
      When Stripe reports at second 400 that the invoice of the subscription "sub_b3" of the customer "cus_b3" was paid, the subscription being active
      Then the subscription of the user is active

    Scenario: An event of one customer leaves the subscription of another user alone
      Given another user with a password pays, as the customer "cus_other" with the active subscription "sub_other"
      And Stripe reports at second 0 that the customer "cus_x" checked out with the active subscription "sub_x"
      When Stripe reports at second 200 that the subscription "sub_x" of the customer "cus_x" was deleted
      Then the subscription of the user is canceled
      And the user is on the Free plan
      And the subscription of the other user is active
      And the other user is on the Paid plan

  Rule: The nightly reconciliation finds what the events missed

    Background:
      Given the app lets users sign in with a password or a passkey
      And a user with a password exists

    Scenario: A subscription that Stripe holds for a known customer is adopted
      Given Stripe reports at second 0 that the customer "cus_adopt" checked out, without any subscription yet
      Then the subscription is linked to the customer "cus_adopt", without any subscription id
      And the user is on the Free plan
      Given Stripe holds the subscription "sub_adopt" of the customer "cus_adopt", active
      When the reconcile job runs
      Then the reconcile job adopted 1 and refreshed 0 subscriptions
      And the subscription is linked to the customer "cus_adopt" and the subscription "sub_adopt"
      And the user is on the Paid plan

    Scenario: A canceled subscription that Stripe holds is not adopted
      Given Stripe reports at second 0 that the customer "cus_no_adopt" checked out, without any subscription yet
      And Stripe holds the subscription "sub_no_adopt" of the customer "cus_no_adopt", canceled
      When the reconcile job runs
      Then the reconcile job adopted 0 subscriptions
      And the subscription is linked to the customer "cus_no_adopt", without any subscription id
      And the user is on the Free plan

    Scenario: The pruning removes the events received more than the retention window ago, and a dry run removes nothing
      Given Stripe events were received 40 days ago and just now
      When the Stripe events are pruned, as a dry run
      Then the pruning removed 0 events
      When the Stripe events are pruned
      Then the pruning removed 1 event
      And only the event of 40 days ago is gone
