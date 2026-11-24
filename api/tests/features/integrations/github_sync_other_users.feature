Feature: Syncing GitHub notifications of several users
  A sync only touches the notifications of the user who syncs: a thread GitHub no
  longer reports is deleted for that user, and never for anybody else.

  @legacy:test_sync_github_notifications::test_sync_notifications_should_mark_deleted_notification_without_subscription
  Scenario: A thread GitHub no longer reports is deleted for its user only
    Given the app lets users sign in with a password or a passkey
    And GitHub reports the notifications 123 and 456
    And GitHub knows pull request 123
    And another user, "jane@doe.net", signed up with a password and signed in
    And that other user is connected to GitHub
    And that other user has the unread GitHub notification "theirs" of the thread 789
    And that other user has the unread GitHub notification "theirs too" of the thread 456
    And I have signed up with the email "john@doe.net" and a password
    And I am connected to GitHub
    And I already have the notifications GitHub reports
    And I have the notification "extra" of the thread 789, which GitHub no longer reports
    When I sync my GitHub notifications
    Then my notifications are the threads GitHub reported
    And they carry what GitHub reported, with the details of pull request 123
    And "extra" is deleted
    And the notification "theirs" of that other user is still unread and unchanged
    And the notification "theirs too" of that other user is still unread and unchanged
