Feature: Scheduling GitHub notification syncs
  A sync can run in the background, started by listing the notifications or by the
  sync endpoint. A sync that follows another too closely does not run, and a
  connection that cannot sync, because it is not validated or has its sync disabled,
  is skipped; a failing one records its failures and ends up failing.

  Background:
    Given I am signed in

  Rule: Syncing in the background

    Scenario: Listing the notifications starts a background sync, and the next one is too soon to run
      Given I am connected to GitHub
      And GitHub reports the notifications 123 and 456
      And GitHub knows pull request 123
      And GitHub serves what it reported
      And I already have the notification "mine" of the GitHub thread 456
      And "mine" is unread in Universal Inbox
      When I list my read notifications, asking for a sync
      Then none of my notifications is read yet
      And within two seconds, one of my notifications becomes read, listing them asking for a sync each time
      Given "mine" is unread in Universal Inbox
      When I ask for an asynchronous sync of my GitHub notifications
      And the background jobs are all done
      Then I have no read notification

    Scenario: The sync endpoint starts a background sync, and the next one is too soon to run
      Given I am connected to GitHub
      And GitHub reports the notifications 123 and 456
      And GitHub knows pull request 123
      And GitHub serves what it reported
      And I already have the notification "mine" of the GitHub thread 456
      And "mine" is unread in Universal Inbox
      When I ask for an asynchronous sync of my GitHub notifications
      Then the app answers 201
      And within two seconds, one of my notifications becomes read, listing them without asking for a sync
      Given "mine" is unread in Universal Inbox
      When I ask for an asynchronous sync of my GitHub notifications
      And the background jobs are all done
      Then I have no read notification

  Rule: Connections that cannot sync

    Scenario: A connection that was never validated is not synced
      Given I have a GitHub connection that was never validated
      And GitHub would answer 200 to anything
      When I ask for a synchronous sync of my GitHub notifications
      Then the app answers 200
      And GitHub was not asked anything

    Scenario: A connection whose synchronisation is disabled is not synced
      Given I have a GitHub connection whose synchronisation is disabled
      And GitHub would answer 200 to anything
      When I ask for a synchronous sync of my GitHub notifications
      Then the app answers 200
      And GitHub was not asked anything

    Scenario: A background sync that fails is recorded, leaves my notifications alone, and the connection ends up failing
      Given my GitHub connection already failed 5 times, the first failure being 2 hours old
      And GitHub reports the notifications 123 and 456
      And I already have the notification "mine" of the GitHub thread 456
      And "mine" is unread in Universal Inbox
      And GitHub answers 400 to anything
      When I ask for an asynchronous sync of my GitHub notifications
      Then the app answers 201
      When the background jobs have all ended
      Then of the 1 job queued, 0 are done, 0 are dead and 1 waits for a retry
      Then I have no read notification
      And my GitHub connection recorded a failed sync, the sixth, and is failing for too many sync failures
