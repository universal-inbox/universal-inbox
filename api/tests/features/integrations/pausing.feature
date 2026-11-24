Feature: Pausing connections
  Providers keep a grant valid as long as its token is, so the connections of
  users who stayed away for too long, and those failing for too long, are
  paused: their grant is revoked at the provider and their credential deleted.
  A paused connection stays paused until the user reconnects it. Inactive users
  are warned by email before the pause, and every user is emailed once paused.
  Connections are paused after 90 days of inactivity (warned 7 days before) or
  30 days of failures.

  Background:
    Given I am signed in

  Rule: The connections of inactive users are paused

    @legacy:test_pause_integration_connections::test_slack_connection_is_paused_past_the_inactivity_threshold::case_1_just_past_the_threshold
    Scenario: A user inactive for just over 90 days has their connection paused
      Given I have a Slack connection syncing my notifications
      And I was last active 90 days and 1 hour ago
      And Slack revokes my grant when asked
      When the pause job runs
      Then my Slack connection is paused for inactivity
      And Slack was asked once to revoke my grant
      And my Slack connection no longer holds a credential
      And I got 1 pause email
      And my last pause email says that my Slack connection was paused for inactivity

    @legacy:test_pause_integration_connections::test_slack_connection_is_paused_past_the_inactivity_threshold::case_2_just_before_the_threshold
    Scenario: A user inactive for just under 90 days keeps their connection
      Given I have a Slack connection syncing my notifications
      And I was last active 89 days and 23 hours ago
      And Slack revokes my grant when asked
      When the pause job runs
      Then my Slack connection is still validated, and was never paused
      And Slack was not asked to revoke my grant
      And my Slack connection still holds its credential
      And I got no pause email

    @legacy:test_pause_integration_connections::test_healthy_connection_of_active_user_is_not_paused
    Scenario: A healthy connection of an active user is never paused
      Given I have a Slack connection syncing my notifications
      And Slack revokes my grant when asked
      When only the connections of inactive users are paused
      Then the pause job reports 0 connections paused and 0 failures
      When only the long-failing connections are paused
      Then the pause job reports 0 connections paused and 0 failures
      And Slack was not asked to revoke my grant
      And my Slack connection is still validated, and was never paused

  Rule: An expired token is renewed before it is revoked, and a refused revocation is retried

    @legacy:test_pause_integration_connections::test_expired_access_token_is_refreshed_before_being_revoked
    Scenario: The fresh Slack token is the one revoked
      Given I have a Slack connection syncing my notifications, with the refresh token "xoxe-1-old-refresh-token"
      And its access token expired an hour ago
      And I was last active 91 days ago
      And Slack renews my token when asked
      And Slack accepts to revoke my renewed token
      When only the connections of inactive users are paused
      Then the pause job reports 1 connections paused and 0 failures
      And Slack was asked once to renew my token
      And Slack was asked once to revoke my grant
      And my Slack connection is paused for inactivity
      And my Slack connection no longer holds a credential

    @legacy:test_pause_integration_connections::test_failed_revocation_is_queued_for_retry
    Scenario: Slack refusing the revocation does not stop the pause
      Given I have a Slack connection syncing my notifications
      And I was last active 91 days ago
      And Slack answers that revoking is rate limited
      When only the connections of inactive users are paused
      Then the pause job reports 1 connections paused and 0 failures
      And Slack was asked once to revoke my grant
      And my Slack connection is paused for inactivity
      And my Slack connection no longer holds a credential
      And one revocation of my Slack grant is waiting to be retried

    @legacy:test_pause_integration_connections::test_dead_refresh_token_counts_as_revoked
    Scenario: A refresh token Slack rejects means the grant is already dead
      Given I have a Slack connection syncing my notifications, with the refresh token "xoxe-1-dead-refresh-token"
      And its access token expired an hour ago
      And my Slack connection has been failing for 31 days, since a token refresh failed
      And Slack rejects my refresh token
      And Slack accepts any revocation
      When only the long-failing connections are paused
      Then the pause job reports 1 connections paused and 0 failures
      And Slack was not asked to revoke my grant
      And my Slack connection is paused

  Rule: A paused connection stays paused until it is reconnected

    @legacy:test_pause_integration_connections::test_paused_connection_is_skipped_by_later_runs_and_token_refresh
    Scenario: Later runs and the token refresh leave a paused connection alone
      Given I have a Slack connection syncing my notifications, with the refresh token "xoxe-1-refresh-token"
      And I was last active 91 days ago
      And Slack revokes my grant when asked
      When only the connections of inactive users are paused
      Then the pause job reports 1 connections paused and 0 failures
      When only the connections of inactive users are paused
      Then the pause job reports 0 connections paused and 0 failures
      And Slack was asked once to revoke my grant
      When the tokens expiring within a year are refreshed
      Then the pause job reports 0 connections paused and 0 failures

    @legacy:test_pause_integration_connections::test_disconnecting_a_paused_connection_clears_the_pause
    Scenario: Disconnecting a paused connection clears the pause
      Given I have a Slack connection syncing my notifications
      And I was last active 91 days ago
      And Slack revokes my grant when asked
      When only the connections of inactive users are paused
      Then the pause job reports 1 connections paused and 0 failures
      When I disconnect it
      Then it is no longer paused

  Rule: Connections failing for too long are paused

    @legacy:test_pause_integration_connections::test_slack_connection_is_paused_past_the_failing_threshold::case_1_by_syncs_just_past_the_threshold
    Scenario: A connection whose syncs failed for just over 30 days is paused
      Given I have a Slack connection syncing my notifications
      And my Slack connection has been failing for 30 days and 1 hour, through its syncs
      And Slack revokes my grant when asked
      When the pause job runs
      Then my Slack connection is paused for a long failure
      And Slack was asked once to revoke my grant
      And my Slack connection no longer holds a credential
      And my Slack connection keeps its notifications in my inbox
      And I got 1 pause email
      And my last pause email says that my Slack connection was paused for a long failure

    @legacy:test_pause_integration_connections::test_slack_connection_is_paused_past_the_failing_threshold::case_2_by_syncs_just_before_the_threshold
    Scenario: A connection whose syncs failed for just under 30 days is left failing
      Given I have a Slack connection syncing my notifications
      And my Slack connection has been failing for 29 days and 23 hours, through its syncs
      And Slack revokes my grant when asked
      When the pause job runs
      Then my Slack connection is still failing
      And Slack was not asked to revoke my grant
      And my Slack connection still holds its credential

    @legacy:test_pause_integration_connections::test_slack_connection_is_paused_past_the_failing_threshold::case_3_by_token_refresh_past_the_threshold
    Scenario: A connection whose token refresh failed over 30 days ago is paused
      Given I have a Slack connection syncing my notifications
      And my Slack connection has been failing for 30 days and 1 hour, since a token refresh failed
      And Slack revokes my grant when asked
      When the pause job runs
      Then my Slack connection is paused for a long failure
      And Slack was asked once to revoke my grant
      And my Slack connection no longer holds a credential
      And my Slack connection keeps its notifications in my inbox
      And I got 1 pause email
      And my last pause email says that my Slack connection was paused for a long failure

    @legacy:test_pause_integration_connections::test_old_connection_that_just_failed_is_not_paused
    Scenario: An old connection that has only just started failing is not paused
      Given I have a Slack connection syncing my notifications
      And my Slack connection was last updated 60 days ago
      And Slack revokes my grant when asked
      When my Slack connection turns failing with the message "Refresh token rejected"
      And the pause job runs
      Then my Slack connection is still failing
      And Slack was not asked to revoke my grant

    @legacy:test_pause_integration_connections::test_linear_connection_is_paused_past_the_failing_threshold::case_1_past_the_threshold
    Scenario: A Linear connection failing for over 30 days is paused too
      Given I have a Linear connection syncing my tasks
      And my Linear connection has been failing for 30 days and 1 hour, through its syncs
      And Linear revokes my grant when asked
      When the pause job runs
      Then my Linear connection is paused for a long failure
      And Linear was asked once to revoke my grant
      And my Linear connection no longer holds a credential
      And I got 1 pause email
      And my last pause email says that my Linear connection was paused for a long failure

    @legacy:test_pause_integration_connections::test_linear_connection_is_paused_past_the_failing_threshold::case_2_before_the_threshold
    Scenario: A Linear connection failing for under 30 days is left failing
      Given I have a Linear connection syncing my tasks
      And my Linear connection has been failing for 29 days and 23 hours, through its syncs
      And Linear revokes my grant when asked
      When the pause job runs
      Then my Linear connection is still failing
      And Linear was not asked to revoke my grant
      And my Linear connection still holds its credential

    @legacy:test_pause_integration_connections::test_long_failing_connections_of_a_user_get_one_email
    Scenario: A user whose connections keep failing is emailed once for all of them
      Given I have a Slack connection syncing my notifications
      And I have a Linear connection syncing my tasks
      And my Slack connection has been failing for 31 days, through its syncs
      And my Linear connection has been failing for 31 days, through its syncs
      And Slack revokes my grant when asked
      And Linear revokes my grant when asked
      When only the long-failing connections are paused
      Then the pause job reports 2 connections paused and 0 failures
      And I got 1 pause email
      And my last pause email says that my Linear and Slack connections were paused for a long failure

  Rule: Inactive users are warned before the pause

    @legacy:test_pause_integration_connections::test_inactive_user_is_warned_once_before_the_pause::case_1_just_past_the_warning_threshold
    Scenario: A user inactive for just over 83 days is warned once
      Given I have a Slack connection syncing my notifications
      And I was last active 83 days and 1 hour ago
      And Slack revokes my grant when asked
      When the pause job runs, warning inactive users 7 days ahead
      And the pause job runs, warning inactive users 7 days ahead
      Then I was warned once that my Slack connection would be paused, after 83 days of inactivity
      And my Slack connection is still validated, and was never paused
      And Slack was not asked to revoke my grant

    @legacy:test_pause_integration_connections::test_inactive_user_is_warned_once_before_the_pause::case_2_just_before_the_warning_threshold
    Scenario: A user inactive for just under 83 days is not warned
      Given I have a Slack connection syncing my notifications
      And I was last active 82 days and 23 hours ago
      And Slack revokes my grant when asked
      When the pause job runs, warning inactive users 7 days ahead
      And the pause job runs, warning inactive users 7 days ahead
      Then I got no pause email
      And my Slack connection is still validated, and was never paused
      And Slack was not asked to revoke my grant

    @legacy:test_pause_integration_connections::test_inactive_user_is_paused_only_after_the_warning_period
    Scenario: A user already inactive for too long is warned first, then paused after the warning period
      Given I have a Slack connection syncing my notifications
      And I was last active 100 days ago
      And Slack revokes my grant when asked
      When the pause job runs, warning inactive users 7 days ahead
      Then my Slack connection is still validated, and was never paused
      And I got 1 pause email
      And my first pause email is a warning
      When the warning I received is 6 days and 23 hours old
      When the pause job runs, warning inactive users 7 days ahead
      Then my Slack connection is still validated, and was never paused
      And I got 1 pause email
      When the warning I received is 7 days and 1 hour old
      When the pause job runs, warning inactive users 7 days ahead
      Then my Slack connection is paused for inactivity
      And Slack was asked once to revoke my grant
      And I got 2 pause emails
      And my last pause email says that my Slack connection was paused for inactivity

    @legacy:test_pause_integration_connections::test_warning_sent_before_the_last_activity_does_not_count
    Scenario: A warning sent before the last activity does not count for a new period of inactivity
      Given I have a Slack connection syncing my notifications
      And the warning I received is 200 days old
      And I was last active 100 days ago
      And Slack revokes my grant when asked
      When the pause job runs, warning inactive users 7 days ahead
      Then my Slack connection is still validated, and was never paused
      And Slack was not asked to revoke my grant
      And I got 1 pause email
      And my first pause email is a warning

    @legacy:test_pause_integration_connections::test_inactive_user_gets_one_email_for_all_their_connections
    Scenario: An inactive user gets one warning and one pause email for all their connections
      Given I have a Slack connection syncing my notifications
      And I have a Linear connection syncing my tasks
      And Slack revokes my grant when asked
      And Linear revokes my grant when asked
      And I was last active 83 days and 1 hour ago
      When the pause job runs, warning inactive users 7 days ahead
      Then I was warned once that my Linear and Slack connections would be paused, after 83 days of inactivity
      And my Slack connection is still validated, and was never paused
      And my Linear connection is still validated, and was never paused
      When I was last active 90 days and 1 hour ago
      When the pause job runs
      Then my Slack connection is paused for inactivity
      And my Linear connection is paused for inactivity
      And my Slack connection no longer holds a credential
      And my Linear connection no longer holds a credential
      And Slack was asked once to revoke my grant
      And Linear was asked once to revoke my grant
      And I got 2 pause emails
      And my last pause email says that my Linear and Slack connections were paused for inactivity

    @legacy:test_pause_integration_connections::test_testing_account_is_paused_without_emails
    Scenario: A testing account is paused on schedule, but never emailed
      Given I have a Slack connection syncing my notifications
      And my account is a testing account
      And I was last active 100 days ago
      And Slack revokes my grant when asked
      When the pause job runs, warning inactive users 7 days ahead
      Then my Slack connection is still validated, and was never paused
      When the warning I received is 8 days old
      When the pause job runs, warning inactive users 7 days ahead
      Then my Slack connection is paused
      And I got no pause email

  Rule: Pausing without email, before the cron is enabled

    @legacy:test_pause_integration_connections::test_pause_without_email_pauses_the_connections_of_inactive_users
    Scenario: The connections of every inactive user are paused, whatever the state of their failures
      Given I have a Slack connection syncing my notifications
      And I have a Linear connection syncing my tasks
      And my Linear connection has been failing for 1 day, through its syncs
      And I was last active 30 days ago
      And Slack revokes my grant when asked
      And Linear revokes my grant when asked
      When the connections of users inactive since yesterday are paused without email
      Then the report counts 1 inactive and 1 failing connections paused
      And my Slack connection is paused for inactivity
      And my Slack connection has its notifications set aside
      And my Linear connection is paused for a long failure
      And Slack was asked once to revoke my grant
      And Linear was asked once to revoke my grant
      And my Slack connection no longer holds a credential
      And my Linear connection no longer holds a credential
      And I got no pause email
      When I was last active 365 days ago
      When the pause job runs, warning inactive users 7 days ahead
      Then I got no pause email

    @legacy:test_pause_integration_connections::test_pause_without_email_pauses_only_long_failing_connections_of_active_users::case_1_failing_past_the_threshold
    Scenario: Only a long-failing connection of an active user is paused
      Given I have a Slack connection syncing my notifications
      And I have a Linear connection syncing my tasks
      And my Linear connection has been failing for 31 days, through its syncs
      And Slack revokes my grant when asked
      And Linear revokes my grant when asked
      When the connections of users inactive since yesterday are paused without email
      Then the report counts 0 inactive and 1 failing connections paused
      And my Slack connection is still validated, and was never paused
      And Slack was not asked to revoke my grant
      And my Linear connection is paused for a long failure
      And Linear was asked once to revoke my grant
      And I got no pause email

    @legacy:test_pause_integration_connections::test_pause_without_email_pauses_only_long_failing_connections_of_active_users::case_2_failing_before_the_threshold
    Scenario: A connection of an active user failing for under 30 days is left alone
      Given I have a Slack connection syncing my notifications
      And I have a Linear connection syncing my tasks
      And my Linear connection has been failing for 29 days, through its syncs
      And Slack revokes my grant when asked
      And Linear revokes my grant when asked
      When the connections of users inactive since yesterday are paused without email
      Then the report counts 0 inactive and 0 failing connections paused
      And my Slack connection is still validated, and was never paused
      And Slack was not asked to revoke my grant
      And my Linear connection is still failing
      And Linear was not asked to revoke my grant
      And I got no pause email

    @legacy:test_pause_integration_connections::test_pause_without_email_dry_run_changes_nothing
    Scenario: A dry run reports what would be paused and changes nothing
      Given I have a Slack connection syncing my notifications
      And I was last active 30 days ago
      And Slack revokes my grant when asked
      When the connections of users inactive since yesterday are checked for a pause without email, as a dry run
      Then the report counts 1 inactive and 0 failing connections paused
      And my Slack connection is still validated, and was never paused
      And Slack was not asked to revoke my grant
      And my Slack connection still holds its credential

    @legacy:test_pause_integration_connections::test_pause_without_email_rejects_a_future_inactivity_date
    Scenario: A date in the future is refused
      When the connections of users inactive since tomorrow are paused without email
      Then the request is refused
