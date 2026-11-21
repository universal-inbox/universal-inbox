Feature: Selecting and acting on notifications in the browser
  The address bar always targets the selected notification, so a notification
  can be shared and opened again by its address. Keyboard shortcuts act on the
  selected notification.

  Background:
    Given I have an account with sample notifications and integrations
    And I am signed in with my notifications listed

  @legacy:test_notification_actions::test_selecting_notification_updates_url
  Scenario: Selecting a notification updates the address bar
    When I click the first notification
    Then it is selected and the address bar shows it
    When I click the second notification
    Then the address bar moves to another notification
    When I press "ArrowDown" to select the next one
    Then the address bar moves to another notification
    When I press "d" to delete it
    Then the address bar moves to another notification

  @legacy:test_notification_actions::test_deeplink_other_section_notification_is_selected
  Scenario: Opening the address of a notification from another section selects it
    When I click the first notification
    Then it is selected and the address bar shows it
    When I press "s" to snooze it
    Then the server has snoozed it
    When I open its address directly
    Then a notification is selected
    And the address bar still shows its address

  @legacy:test_notification_actions::test_switching_section_updates_url
  Scenario: Switching to another section selects its first notification
    When I click the first notification
    Then it is selected and the address bar shows it
    When I press "s" to snooze it
    Then the server has snoozed it
    When I open the Snoozed section
    Then the address bar shows a notification

  @legacy:test_notification_actions::test_act_on_notifications_with_keyboard
  Scenario: Deleting, unsubscribing from and snoozing notifications with the keyboard
    Given I have at least 3 notifications listed
    When I click the first notification
    Then it is selected and the address bar shows it
    When I press "d" to delete it
    Then it disappears from the list
    And the server has deleted it
    And the address bar moves to another notification
    When I press "u" to unsubscribe from it
    Then it disappears from the list
    And the server has unsubscribed from it
    And the address bar moves to another notification
    When I press "s" to snooze it
    Then it disappears from the list
    And the server has snoozed it
    And the address bar moves to another notification
