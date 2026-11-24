Feature: Syncing Slack threads
  A sync reads again the Slack threads the user has a notification for: a thread the
  user read in Slack is deleted, a thread the user unsubscribed from in Slack is
  unsubscribed, and an unchanged thread stays unread. A sync also learns the team of
  the connection.

  Background:
    Given I am signed in
    And I am connected to Slack, syncing its notifications

  Scenario Outline: A thread that is <state> in Slack
    Given I already have an unread notification of a Slack thread
    And that thread is <state> in Slack
    When I sync my Slack notifications
    Then I get one Slack notification, <status>

    @legacy:test_sync_slack_threads::sync_slack_thread_notifications::test_sync_slack_thread_marked_as_read
    Examples:
      | state | status  |
      | read  | deleted |

    @legacy:test_sync_slack_threads::sync_slack_thread_notifications::test_sync_slack_thread_marked_as_unsubscribed
    Examples:
      | state        | status       |
      | unsubscribed | unsubscribed |

    @legacy:test_sync_slack_threads::sync_slack_thread_notifications::test_sync_slack_thread_no_change
    Examples:
      | state     | status |
      | unchanged | unread |

  @legacy:test_sync_slack_threads::sync_slack_thread_notifications::test_sync_slack_threads_stores_missing_team_context
  Scenario: A sync stores the team the connection was missing
    Given Slack tells me that my team is "T053BSKET"
    When I sync my Slack notifications
    Then the sync returned no notification
    And my Slack connection stores the team context of that team
