Feature: The integration limit of the Free plan
  A Free user can keep a limited number of validated integrations. A user over the limit gets a grace period,
  then the excess integrations are paused, and restored when the user pays.

  Background:
    Given the app lets users sign in with a password or a passkey
    And a user with a password exists

  Rule: Counting integrations

    @legacy:test_billing::count_validated_matches_fetch_all_len
    Scenario: Counting the validated integrations gives the number of rows that load one by one
      Given the user validates every integration that syncs notifications
      When the validated integrations of the user are counted
      Then the usage of the user is 3
      And that count is the number of validated integrations that load one by one

    @legacy:test_billing::count_validated_excludes_api_connection
    Scenario: The implicit API connection does not count
      Given the user validates a Todoist integration and the implicit API connection
      When the validated integrations of the user are counted
      Then the usage of the user is 1


    Scenario: A real integration is allowed one below the limit
      Given the user validates one integration fewer than the Free plan allows
      When the user adds a Todoist integration
      Then the cap allows it
    @legacy:test_billing::the_cap_does_not_refuse_the_implicit_api_connection
    Scenario: The cap refuses a real integration at the limit, but not the implicit API connection
      Given the user validates as many integrations as the Free plan allows
      When the user adds the implicit API connection
      Then the cap allows it
      When the user adds a GitHub integration
      Then the cap refuses it, asking for a payment

  Rule: A user over the limit gets a grace period

    @legacy:test_billing::over_limit_free_user_arms_grace_idempotently_and_is_reconcilable
    Scenario: The grace deadline of a Free user over the limit is armed once, and enforced when it expires
      Given the user validates one more integration than the Free plan allows
      Then the user has no subscription
      When the over-limit grace deadline of the user is armed
      And the over-limit grace deadline of the user is armed
      Then arming it a second time gives the same deadline
      And the subscription of the user is incomplete
      And the user is on the Free plan
      When the grace deadline of the user is set to second 1
      And the reconcile job runs
      Then the reconcile job enforced 1 expired grace deadline
      And 1 integration of the user is paused, with the sync switched off and the previous config kept

    @legacy:test_billing::returning_under_cap_clears_armed_grace_deadline
    Scenario: Going back under the limit clears the grace deadline
      Given the user validates one more integration than the Free plan allows
      When the over-limit grace deadline of the user is armed
      And the first integration of the user is no longer validated
      And the over-limit grace deadline of the user is armed
      Then the last arming gave no deadline
      And no over-limit grace deadline of the user is armed

    @legacy:test_billing::downgrade_arms_grace_then_reconcile_pauses_then_upgrade_restores
    Scenario: A downgrade arms the grace, the expiry pauses the excess, and an upgrade restores it
      Given the user validates one more integration than the Free plan allows
      And Stripe reports at second 0 that the customer "cus_a" checked out with the active subscription "sub_a"
      Then the user is on the Paid plan
      When Stripe reports at second 200 that the subscription "sub_a" of the customer "cus_a" was deleted
      Then the user is on the Free plan
      And the subscription of the user is canceled
      And the over-limit grace deadline of the user is armed
      And every integration of the user is still enabled and not paused
      When the reconcile job runs
      Then the reconcile job enforced 0 expired grace deadlines
      And every integration of the user is still enabled and not paused
      When the grace deadline of the user is set to second 1
      And the reconcile job runs
      Then the reconcile job enforced 1 expired grace deadline
      And 1 integration of the user is paused, with the sync switched off and the previous config kept
      And the integrations that the Free plan allows still sync notifications, and the others do not
      When the usage of the user is counted, and the grace deadline is armed again
      Then the usage of the user is the number of integrations that the Free plan allows
      And the last arming gave no deadline
      And no over-limit grace deadline of the user is armed
      When Stripe reports at second 400 that the subscription "sub_a" of the customer "cus_a" was updated to active
      Then the user is on the Paid plan
      And no over-limit grace deadline of the user is armed
      And every integration of the user is restored to its enabled config, without a pause marker or a snapshot

    Scenario: The grace period and the enforcement of one user do not touch another user
      Given the user validates one more integration than the Free plan allows
      And another user with a password validates every integration that syncs notifications
      When the over-limit grace deadline of the user is armed
      And the over-limit grace deadline of the other user is armed
      And the Free plan is enforced on the user
      Then the enforcement paused 1 integration
      And none of the integrations of the other user is paused, and all of them are still validated
      And the usage of the other user is 3
      When the grace deadline of the user is set to second 1
      And the reconcile job runs
      Then the reconcile job enforced 1 expired grace deadline
      And none of the integrations of the other user is paused, and all of them are still validated

  Rule: The enforcement pauses only what it must, and what it can restrict

    @legacy:test_billing::a_connection_a_pause_cannot_restrict_keeps_counting_against_the_cap
    Scenario: A connection whose sync cannot be switched off keeps counting against the limit
      Given the user validates as many integrations as the Free plan allows
      And the user validates a Google Calendar integration, whose sync a pause cannot switch off
      When the Free plan is enforced on the user
      Then the enforcement paused 1 integration
      And the paused integration is one whose sync a pause can switch off
      And the Google Calendar connection is not marked as paused
      When the usage of the user is counted, and the grace deadline is armed again
      Then the usage of the user is the number of integrations that the Free plan allows
      And the last arming gave no deadline

    @legacy:test_billing::enforcement_run_twice_keeps_the_pre_pause_snapshot
    Scenario: A second enforcement keeps the config that the user had before the pause
      Given the user validates one more integration than the Free plan allows
      When the Free plan is enforced on the user
      Then the enforcement paused 1 integration
      And the paused integration keeps the config that the user had, not the disabled one
      When the Free plan is enforced on the user
      Then the enforcement paused 0 integrations
      And the paused integration still keeps the same config

    @legacy:test_billing::enforcement_does_not_pause_a_user_already_at_the_cap
    Scenario: A user at the limit, with an older paused connection, loses nothing more
      Given the user validates as many integrations as the Free plan allows
      And the user validates a Google Calendar integration, whose sync a pause cannot switch off
      And an older enforcement marked that Google Calendar connection as paused
      When the validated integrations of the user are counted
      Then the usage of the user is the number of integrations that the Free plan allows
      When the Free plan is enforced on the user
      Then the enforcement paused 0 integrations
      When the validated integrations of the user are counted
      Then the usage of the user is the number of integrations that the Free plan allows
