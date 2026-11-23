Feature: Exporting the account data
  A signed-in user downloads all their data as a file that contains no
  secret. The export holds only the user's own data and is limited per user.

  @legacy:test_export_user_data::test_export_requires_authentication
  Scenario: A visitor who is not signed in cannot export data
    When a visitor who is not signed in exports their data
    Then the app answers 401

  @legacy:test_export_user_data::test_export_contains_user_data_without_secrets
  Scenario: The export holds the user's data without any secret and nobody else's
    Given I am signed in
    And I have an API token
    And I am connected to GitHub
    And I have a GitHub notification
    And I am connected to Todoist
    And I am connected to Linear, syncing its tasks into the project "Project1"
    And I have a task synced from Linear
    And another user is signed in
    And that other user has an API token
    And that other user is connected to GitHub
    When I export my data
    Then the app answers 200
    And the response is a file download whose name starts with "universal-inbox-export-"
    And the export contains neither my API token nor the GitHub access token
    And the export is about me and has no chat support email signature
    And the export lists exactly my API token
    And the export lists exactly my GitHub, Todoist and Linear connections and none of the other user's
    And the export lists exactly my notification
    And the export lists exactly my task

  @legacy:test_export_user_data::test_export_has_a_per_user_budget
  Scenario: Exporting is limited per user
    Given I am signed in
    When I export my data 2 times, each answered 200
    And I export my data once more
    Then the app answers 429
    And the response has a "Retry-After" header
