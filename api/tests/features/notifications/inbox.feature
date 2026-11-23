Feature: The notification inbox
  A signed-in user lists their notifications by status, source and snooze, in a
  stable order that pages never skip, reads one notification, and changes it.
  Nobody sees or changes the notifications of another user.

  Background:
    Given I am signed in

  Rule: Listing notifications

    @legacy:test_notifications::list_notifications::test_empty_list_notifications
    Scenario: A user without notifications sees none
      When I list my unread notifications
      Then I see no notifications

    @legacy:test_notifications::list_notifications::test_list_notifications
    Scenario: Notifications are listed by status, snoozed ones apart, and never to another user
      Given I am connected to GitHub
      And I have a GitHub notification called "review request"
      And I have a GitHub notification called "mention", which I marked as read
      And I have a GitHub notification called "old", which I deleted
      And I have a GitHub notification called "later", snoozed until 1 minutes from now
      And another user is signed in
      When I list my unread notifications
      Then I see only the notification "review request"
      When I list my read and unread notifications
      Then I see these notifications, ordered by update time and then by id:
        | review request |
        | mention        |
      When I list my read and unread notifications, snoozed ones included
      Then I see these notifications, ordered by update time and then by id:
        | review request |
        | mention        |
        | later          |
      When I list my deleted notifications
      Then I see only the notification "old"
      When I list my unsubscribed notifications
      Then I see no notifications
      When another user lists their unread notifications
      Then I see no notifications

    @legacy:test_notifications::list_notifications::test_list_notifications_with_same_updated_at_in_stable_order
    Scenario Outline: Notifications updated at the same second keep one stable order in <order> order
      Given I am connected to GitHub
      And I have 5 GitHub notifications updated in the same second
      When I list my unread notifications in "<order>" order 3 times
      Then in "<order>" order, the page after the 2nd of them lists the other 3 and links back to the 3rd
      And in "<order>" order, the page before the 4th of them lists the first 3 and links forward to the 3rd
      And in "<order>" order, a page token that holds only a time one second before them lists all of them

      Examples:
        | order      |
        | ascending  |
        | descending |

    @legacy:test_notifications::list_notifications::test_list_only_snoozed_and_unsnooze_and_undelete
    Scenario: Only snoozed notifications are listed apart, and snoozing or deleting can be undone
      Given I am connected to GitHub
      And I have a GitHub notification called "untouched"
      And I have a GitHub notification called "later", snoozed until 10 minutes from now
      And I have a GitHub notification called "old", which I deleted
      When I list my snoozed unread and read notifications
      Then the only snoozed notification I see is "later"
      When I unsnooze "later"
      And I list my snoozed unread and read notifications
      Then I see no snoozed notifications
      When I list my unread notifications
      Then "untouched" is among the notifications I see
      And "later" is among the notifications I see
      When I undelete "old"
      And I list my unread notifications
      Then "old" is among the notifications I see
      When I list my deleted notifications
      Then I see no notifications

    @legacy:test_notifications::list_notifications::test_list_notifications_filtered_by_kind
    Scenario: Notifications are filtered by their source
      Given I am connected to GitHub
      And I am connected to Linear
      And I have a GitHub notification called "from GitHub"
      And I have a Linear notification called "from Linear"
      When I list my read and unread notifications of GitHub
      Then I see only the notification "from GitHub"
      When I list my read and unread notifications of Linear
      Then I see only the notification "from Linear"
      When I list my read and unread notifications of Todoist
      Then I see no notifications

    @legacy:test_notifications::list_notifications::test_list_notifications_skips_undecodable_third_party_item
    Scenario: A notification whose stored item can no longer be decoded is skipped
      Given I am connected to GitHub
      And I have a GitHub notification called "readable"
      And I have a GitHub notification called "broken" whose stored item can no longer be decoded
      When I list my unread notifications
      Then I see only the notification "readable"
      And the stored items of the unread GitHub notifications are only that of "readable"

  Rule: Reading one notification

    @legacy:test_notifications::get_notification::test_get_notification_of_another_user
    Scenario: A user cannot read the notification of another user
      Given I am connected to GitHub
      And I have a GitHub notification called "mine"
      And another user is signed in
      When that other user gets the notification "mine"
      Then the app answers 403
      And the error message is "Forbidden access: Only the owner of the notification <id> can access it", with <id> being the id of "mine"

    @legacy:test_notifications::get_notification::test_get_unknown_notification
    Scenario: A notification that does not exist is not found
      When I get a notification that does not exist
      Then the app answers 404
      And the error message is "Cannot find notification <id>", with <id> being the id of that unknown notification

  Rule: Changing one notification

    @legacy:test_notifications::patch_notification::test_patch_notification_snoozed_until
    Scenario: A user snoozes a notification
      Given I am connected to GitHub
      And I have a GitHub notification called "mine"
      When I snooze "mine" until "2022-01-01T01:02:03Z"
      Then the patched notification is "mine" snoozed until "2022-01-01T01:02:03Z"

    @legacy:test_notifications::patch_notification::test_patch_notification_status_without_modification
    Scenario: A change that changes nothing is not modified
      Given I am connected to GitHub
      And I have a GitHub notification called "mine"
      When I snooze "mine" until "2022-01-01T01:02:03Z"
      And I send "mine" the status it already has and the snooze time "2022-01-01T01:02:03Z"
      Then the app answers 304

    @legacy:test_notifications::patch_notification::test_patch_notification_of_another_user
    Scenario: A user cannot change the notification of another user
      Given I am connected to GitHub
      And I have a GitHub notification called "mine"
      And another user is signed in
      When that other user marks "mine" as deleted
      Then the app answers 403
      And "mine" is still unread

    @legacy:test_notifications::patch_notification::test_patch_notification_without_values_to_update
    Scenario: A change without any value is refused
      Given I am connected to GitHub
      And I have a GitHub notification called "mine"
      When I send "mine" a patch with nothing to change
      Then the app answers 400
      And the error message is "Invalid input data: Missing `status` field value to update notification <id>", with <id> being the id of "mine"

    @legacy:test_notifications::patch_notification::test_patch_unknown_notification
    Scenario: A notification that does not exist cannot be changed
      When I mark a notification that does not exist as deleted
      Then the app answers 404
      And the error message is "Cannot update unknown notification <id>", with <id> being the id of that unknown notification
