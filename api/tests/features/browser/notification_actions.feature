Feature: Selecting and acting on notifications in the browser
  The address bar always targets the selected notification, so a notification
  can be shared and opened again by its address. Keyboard shortcuts act on the
  selected notification.

  Background:
    Given I have an account with sample notifications and integrations
    And I am signed in with my notifications listed

  Scenario: Selecting a notification updates the address bar
    When I click the first notification
    Then it is selected and the address bar shows it
    When I click the second notification
    Then the address bar moves to another notification
    When I press "ArrowDown" to select the next one
    Then the address bar moves to another notification
    When I press "d" to delete it
    Then the address bar moves to another notification

  Scenario: Opening the address of a notification from another section selects it
    When I click the first notification
    Then it is selected and the address bar shows it
    When I press "s" to snooze it
    Then the server has snoozed it
    When I open its address directly
    Then a notification is selected
    And the address bar still shows its address

  Scenario: Switching to another section selects its first notification
    When I click the first notification
    Then it is selected and the address bar shows it
    When I press "s" to snooze it
    Then the server has snoozed it
    When I open the Snoozed section
    Then the address bar shows a notification

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
