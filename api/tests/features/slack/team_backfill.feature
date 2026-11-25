Feature: Backfilling the team of the Slack connections
  The backfill command asks Slack which team each Slack connection without a team belongs
  to, and stores it. It leaves the connections that have a team alone, counts what it cannot
  do instead of stopping, and never changes the status of a connection.

  Background:
    Given I am signed in

  @legacy:test_slack_backfill_team_id::test_backfill_team_id_for_connection_without_context
  Scenario: A connection without a team gets its team, and a second run has nothing left to do
    Given I have a Slack connection that has no team yet
    And Slack tells that the access token of my connection belongs to the team "T053BSKET", and is asked once
    When I run the team backfill
    Then the backfill report says 1 updated, 0 skipped without credential, 0 skipped without access token and 0 failed
    And my Slack connection is in the team "T053BSKET"
    When I run the team backfill again
    Then the backfill report says 0 updated, 0 skipped without credential, 0 skipped without access token and 0 failed
    And my Slack connection is in the team "T053BSKET"

  @legacy:test_slack_backfill_team_id::test_backfill_team_id_dry_run
  Scenario: A dry run reports the update and changes nothing
    Given I have a Slack connection that has no team yet
    And Slack tells that the access token of my connection belongs to the team "T053BSKET", and is asked once
    When I run the team backfill as a dry run
    Then the backfill report says 1 updated, 0 skipped without credential, 0 skipped without access token and 0 failed
    And my Slack connection has no team

  @legacy:test_slack_backfill_team_id::test_backfill_team_id_ignores_connection_with_context
  Scenario: A connection that already has a team is not asked about
    Given I have a Slack connection in the team "T01EXISTING"
    And Slack is not asked about the access token of my connection
    When I run the team backfill
    Then the backfill report says 0 updated, 0 skipped without credential, 0 skipped without access token and 0 failed
    And my Slack connection is in the team "T01EXISTING"

  @legacy:test_slack_backfill_team_id::test_backfill_team_id_skips_connection_without_credential
  Scenario: A connection that never completed its OAuth flow is skipped
    Given I have a Slack connection that never completed its OAuth flow
    And Slack is not asked about the access token of my connection
    When I run the team backfill
    Then the backfill report says 0 updated, 1 skipped without credential, 0 skipped without access token and 0 failed
    And my Slack connection has no team

  @legacy:test_slack_backfill_team_id::test_backfill_team_id_skips_connection_with_expired_token
  Scenario: A connection whose access token expired is counted as failed, and its status is kept
    Given I have a Slack connection whose access token expired an hour ago, and that has no team yet
    And Slack is not asked about the access token of my connection
    When I run the team backfill
    Then the backfill report says 0 updated, 0 skipped without credential, 0 skipped without access token and 1 failed
    And my Slack connection has no team
    And my Slack connection is still validated

  @legacy:test_slack_backfill_team_id::test_backfill_team_id_continues_after_revoked_token
  Scenario: A revoked access token does not stop the backfill of the other connections
    Given I have a Slack connection that has no team yet
    And another user is signed in
    And another user has a Slack connection with a revoked access token, and no team yet
    And Slack tells that the access token of my connection belongs to the team "T053BSKET", and is asked once
    And Slack rejects the access token of that other user as invalid, and is asked once
    When I run the team backfill
    Then the backfill report says 1 updated, 0 skipped without credential, 0 skipped without access token and 1 failed
    And my Slack connection is in the team "T053BSKET"
    And the Slack connection of that other user has no team
