Feature: Scheduling GitHub notification syncs
  A sync can run in the background, started by listing the notifications or by the
  sync endpoint. A sync that follows another too closely does not run, and a
  connection that cannot sync, because it is not validated or has its sync disabled,
  is skipped; a failing one records its failures and ends up failing.

  Background:
    Given I am signed in

  Rule: Syncing in the background

    Scenario Outline: A background sync started <how> brings in GitHub's changes, once
      Given I am connected to GitHub
      And GitHub reports the notifications 123 and 456
      And GitHub knows pull request 123
      And GitHub serves what it reported
      And I already have the notification "mine" of the GitHub thread 456
      And "mine" is unread in Universal Inbox
      When <trigger>
      Then <before>
      And within two seconds, one of my notifications becomes read, <listing>
      Given GitHub would answer 200 to anything
      When I ask for an asynchronous sync of my GitHub notifications
      And one second passes
      Then I still have exactly one read notification

      @legacy:test_sync_github_notifications::test_sync_all_notifications_asynchronously::case_1_trigger_sync_when_listing_notifications
      Examples:
        | how                      | trigger                                           | before                                   | listing                                        |
        | when listing them        | I list my read notifications, asking for a sync   | none of my notifications is read yet    | listing them asking for a sync each time      |

      @legacy:test_sync_github_notifications::test_sync_all_notifications_asynchronously::case_2_trigger_sync_with_sync_endpoint
      Examples:
        | how                      | trigger                                           | before                                   | listing                                        |
        | with the sync endpoint   | I ask for an asynchronous sync of my GitHub notifications | the app answers 201                      | listing them without asking for a sync        |

  Rule: Connections that cannot sync

    @legacy:test_sync_github_notifications::test_sync_all_notifications_with_no_validated_integration_connections
    Scenario: A connection that was never validated is not synced
      Given I have a GitHub connection that was never validated
      And GitHub would answer 200 to anything
      When I ask for a synchronous sync of my GitHub notifications
      Then the app answers 200

    @legacy:test_sync_github_notifications::test_sync_all_notifications_with_synchronization_disabled
    Scenario: A connection whose synchronisation is disabled is not synced
      Given I have a GitHub connection whose synchronisation is disabled
      And GitHub would answer 200 to anything
      When I ask for a synchronous sync of my GitHub notifications
      Then the app answers 200

    @legacy:test_sync_github_notifications::test_sync_all_notifications_asynchronously_in_error
    Scenario: A background sync that fails is recorded, and the connection ends up failing
      Given my GitHub connection already failed 5 times, the first failure being 2 hours old
      And GitHub answers 400 to anything
      When I ask for an asynchronous sync of my GitHub notifications
      Then the app answers 201
      When one second passes
      Then I have no read notification
      And my GitHub connection recorded a failed sync, the sixth, and is failing for too many sync failures
