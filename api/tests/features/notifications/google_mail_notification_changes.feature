Feature: Changing the notification of a Google Mail thread
  Deleting the notification of a thread, or unsubscribing from it, archives the thread in
  Google Mail: the inbox label and the synced label are removed. A snooze stays in the app.
  When Google Mail refuses, the change is refused with a public error and nothing is stored.

  Background:
    Given I am signed in
    And I am connected to Google Mail
    And I have the notification "mine" of the Google Mail thread 123

  @legacy:test_google_mail_notifications::patch_resource::test_patch_google_mail_notification_status_as_deleted
  Scenario: Deleting the notification archives the thread in Google Mail
    Given Google Mail archives, once and with my access token, the thread 123: it removes the inbox label and the synced label, and adds none
    When I set the status of "mine" to deleted
    Then the patched notification is deleted, was read just now, and is otherwise as "mine" was

  @legacy:test_google_mail_notifications::patch_resource::test_patch_google_mail_notification_status_as_unsubscribed
  Scenario: Unsubscribing from the notification only archives the thread in Google Mail
    Given Google Mail archives, once and with my access token, the thread 123: it removes the inbox label and the synced label, and adds none
    When I set the status of "mine" to unsubscribed
    Then the patched notification is unsubscribed, was read just now, and is otherwise as "mine" was

  @legacy:test_google_mail_notifications::patch_resource::test_patch_google_mail_notification_status_as_deleted_with_google_mail_error_response
  Scenario: A refusal of Google Mail is a public internal error, and the notification stays
    Given Google Mail refuses, once, to modify the thread 123
    When I try to delete "mine"
    Then the app answers 500
    And the answer is the public error "An internal error occurred. Please retry later or contact support with the correlation id."
    And "mine", read again, is exactly as it was

  @legacy:test_google_mail_notifications::patch_resource::test_patch_google_mail_notification_snoozed_until
  Scenario: Snoozing the notification asks Google Mail nothing
    Given Google Mail is not asked anything
    When I snooze "mine" until "2022-01-01T01:02:03Z"
    Then the patched notification is "mine" snoozed until "2022-01-01T01:02:03Z"

  Scenario Outline: Another user cannot set my notification to <status>, and their notification of the same thread is left alone
    Given another user is signed in
    And that other user has their own Google Mail connection, and their own notification "theirs" of the same thread
    And Google Mail is not asked anything
    When that other user tries to change the status of "mine" to <status>
    Then the app answers 403
    And "mine", read again, is exactly as it was
    And the notification "theirs" of that other user, read by that other user, is exactly as it was

    Examples:
      | status       |
      | deleted      |
      | unsubscribed |
