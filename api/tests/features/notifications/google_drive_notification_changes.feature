Feature: Changing the notification of a Google Drive comment
  Google Drive has nothing to archive or to unsubscribe from: deleting the notification of a
  comment, or unsubscribing from it, is recorded by the app and Google Drive is asked nothing.

  Background:
    Given I am signed in
    And I am connected to Google Drive
    And I already have the Google Drive notification "mine" of the comment 123

  Scenario: Deleting the notification asks Google Drive nothing
    Given Google Drive is not asked anything
    When I set the status of "mine" to deleted
    Then the patched notification is deleted, was read just now, and is otherwise as "mine" was

  Scenario: Unsubscribing from the notification asks Google Drive nothing
    Given Google Drive is not asked anything
    When I set the status of "mine" to unsubscribed
    Then the patched notification is unsubscribed, was read just now, and is otherwise as "mine" was

  Scenario: Snoozing the notification asks Google Drive nothing
    Given Google Drive is not asked anything
    When I snooze "mine" until "2022-01-01T01:02:03Z"
    Then the patched notification is "mine" snoozed until "2022-01-01T01:02:03Z"

  Scenario Outline: Another user cannot set my notification to <status>, and their notification of the same comment is left alone
    Given another user is signed in
    And that other user has their own Google Drive connection, and their own notification "theirs" of the same comment
    And Google Drive is not asked anything
    When that other user tries to change the status of "mine" to <status>
    Then the app answers 403
    And "mine", read again, is exactly as it was
    And the notification "theirs" of that other user, read by that other user, is exactly as it was

    Examples:
      | status       |
      | deleted      |
      | unsubscribed |
