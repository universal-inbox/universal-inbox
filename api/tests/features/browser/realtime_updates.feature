Feature: Realtime updates across browser tabs
  The app pushes changes to every open tab of the same user, much faster than
  the 60 seconds polling of the list.

  Scenario: A notification deleted in a tab disappears from another tab
    Given I have an account with sample notifications and integrations
    And I am signed in with my notifications listed
    When I open my inbox in a second tab
    Then the second tab lists my notifications and is connected for live updates
    When I click the first notification
    Then it is selected
    When I press "d" to delete it
    Then the second tab shows one fewer notification within 3 seconds
