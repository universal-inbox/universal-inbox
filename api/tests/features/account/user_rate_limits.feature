Feature: Limiting costly requests per user
  Each user has a budget per minute for syncs and for bulk changes of
  notifications. Once it is spent the app refuses with a delay to wait, without
  touching what other users may do.

  Background:
    Given I am signed in

  @legacy:test_user_rate_limits::test_sync_requests_share_a_per_user_budget
  Scenario: Notification syncs and task syncs draw from one budget
    When I ask for 10 syncs, alternating between notifications and tasks, each answered 201
    And I ask for one more task sync
    Then the app answers 429
    And the response asks to wait at least 1 second before retrying
    When another user who is signed in asks for a notifications sync
    Then the app answers 201

  @legacy:test_user_rate_limits::test_bulk_notification_patch_has_a_per_user_budget
  Scenario: Bulk changes of notifications have a budget
    When I delete all my unread notifications 30 times, each answered 200
    And I delete all my unread notifications once more
    Then the app answers 429
