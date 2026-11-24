Feature: Setting notifications aside
  Saving the configuration of a connection saves it and nothing else: no
  notification is removed and none of the triage a user invested in it (read,
  snoozed, turned into a task) is disturbed. Muting or disconnecting a provider
  takes its notifications out of the inbox, but sets them aside instead of
  removing them, and they come back as they were once the sync that reconciles
  the inbox has completed. Only a user's own action sets notifications aside.

  Background:
    Given I am signed in

  Rule: Saving a configuration leaves the inbox as it was

    Scenario Outline: Saving the configuration of my <provider> connection leaves my triaged notification alone
      Given I have a <provider> notification that I read, snoozed and turned into a task
      When I save, one after the other, every change to my <provider> connection that leaves what belongs in my inbox alone
      Then my notification is unchanged in my inbox

      @legacy:test_set_aside_notifications::test_config_update_preserves_triage_state::case_1
      Examples:
        | provider        |
        | GitHub          |

      @legacy:test_set_aside_notifications::test_config_update_preserves_triage_state::case_2
      Examples:
        | provider        |
        | Linear          |

      @legacy:test_set_aside_notifications::test_config_update_preserves_triage_state::case_3
      Examples:
        | provider        |
        | Google Mail     |

      @legacy:test_set_aside_notifications::test_config_update_preserves_triage_state::case_4
      Examples:
        | provider        |
        | Google Drive    |

      @legacy:test_set_aside_notifications::test_config_update_preserves_triage_state::case_5
      Examples:
        | provider        |
        | Google Calendar |

      @legacy:test_set_aside_notifications::test_config_update_preserves_triage_state::case_6
      Examples:
        | provider        |
        | Slack           |

      @legacy:test_set_aside_notifications::test_config_update_preserves_triage_state::case_7
      Examples:
        | provider        |
        | Todoist         |

      @legacy:test_set_aside_notifications::test_config_update_preserves_triage_state::case_8
      Examples:
        | provider        |
        | TickTick        |

  Rule: Muting a provider sets its notifications aside, and unmuting restores them

    Scenario Outline: Muting <provider> sets my notification aside, and unmuting restores it once synced
      Given I have a <provider> notification that I read, snoozed and turned into a task
      When I switch off the notifications of my <provider> connection
      Then my notification is set aside: out of my inbox and its count, but still readable by its id
      When I switch the notifications of my <provider> connection back on
      Then my notification is set aside: out of my inbox and its count, but still readable by its id
      When the notifications sync of <provider> completes
      Then my notification is unchanged in my inbox

      @legacy:test_set_aside_notifications::test_mute_sets_aside_and_unmute_restores::case_1
      Examples:
        | provider     |
        | GitHub       |

      @legacy:test_set_aside_notifications::test_mute_sets_aside_and_unmute_restores::case_2
      Examples:
        | provider     |
        | Linear       |

      @legacy:test_set_aside_notifications::test_mute_sets_aside_and_unmute_restores::case_3
      Examples:
        | provider     |
        | Google Mail  |

      @legacy:test_set_aside_notifications::test_mute_sets_aside_and_unmute_restores::case_4
      Examples:
        | provider     |
        | Google Drive |

      @legacy:test_set_aside_notifications::test_mute_sets_aside_and_unmute_restores::case_5
      Examples:
        | provider     |
        | Slack        |

    Scenario Outline: Switching off the synchronisation of <provider> keeps the notifications it already collected
      Given I have a <provider> notification that I read, snoozed and turned into a task
      When I switch off the synchronisation of my <provider> connection
      Then my notification is unchanged in my inbox

      @legacy:test_set_aside_notifications::test_mute_does_not_set_aside_task_derived_notifications::case_1
      Examples:
        | provider |
        | Todoist  |

      @legacy:test_set_aside_notifications::test_mute_does_not_set_aside_task_derived_notifications::case_2
      Examples:
        | provider |
        | TickTick |

    @legacy:test_set_aside_notifications::test_repeated_mute_does_not_refresh_set_aside_at
    Scenario: Muting again does not refresh the moment the notification was set aside
      Given I have a Slack notification that I read, snoozed and turned into a task
      When my Slack notifications are muted, with nothing set aside before
      And I save another configuration that leaves my Slack notifications switched off
      Then my notification keeps the moment it was set aside

  Rule: Disconnecting a provider sets its notifications aside, and reconnecting restores them

    Scenario Outline: Disconnecting <provider> sets my notification aside, and reconnecting restores it once synced
      Given I have a <provider> notification that I read, snoozed and turned into a task
      When I disconnect my <provider> connection
      Then my notification is set aside: out of my inbox and its count, but still readable by its id
      When I reconnect my <provider> connection
      Then my notification is set aside: out of my inbox and its count, but still readable by its id
      When the sync that restores the <provider> notifications completes
      Then my notification is unchanged in my inbox

      @legacy:test_set_aside_notifications::test_disconnect_sets_aside_and_reconnect_restores::case_1
      Examples:
        | provider        |
        | GitHub          |

      @legacy:test_set_aside_notifications::test_disconnect_sets_aside_and_reconnect_restores::case_2
      Examples:
        | provider        |
        | Linear          |

      @legacy:test_set_aside_notifications::test_disconnect_sets_aside_and_reconnect_restores::case_3
      Examples:
        | provider        |
        | Google Mail     |

      @legacy:test_set_aside_notifications::test_disconnect_sets_aside_and_reconnect_restores::case_4
      Examples:
        | provider        |
        | Google Drive    |

      @legacy:test_set_aside_notifications::test_disconnect_sets_aside_and_reconnect_restores::case_5
      Examples:
        | provider        |
        | Google Calendar |

      @legacy:test_set_aside_notifications::test_disconnect_sets_aside_and_reconnect_restores::case_6
      Examples:
        | provider        |
        | Slack           |

      @legacy:test_set_aside_notifications::test_disconnect_sets_aside_and_reconnect_restores::case_7
      Examples:
        | provider        |
        | Todoist         |

      @legacy:test_set_aside_notifications::test_disconnect_sets_aside_and_reconnect_restores::case_8
      Examples:
        | provider        |
        | TickTick        |

    @legacy:test_set_aside_notifications::test_inactivity_pause_sets_aside_and_reconnect_restores
    Scenario: A connection paused for inactivity sets its notifications aside the same way
      Given I have a Slack notification that I read, snoozed and turned into a task
      And I was last active 100 days ago
      And Slack revokes my grant when asked
      When the pause job pauses the connections of users inactive for over 90 days
      Then my notification is set aside: out of my inbox and its count, but still readable by its id
      When I reconnect my Slack connection
      And the sync that restores the Slack notifications completes
      Then my notification is unchanged in my inbox

    @legacy:test_set_aside_notifications::test_disconnect_does_not_set_aside_api_notifications
    Scenario: Notifications pushed through the API stay in the inbox when their connection is disconnected
      Given I have a notification from a web page sent through the API
      When I disconnect my API connection
      Then my notification is unchanged in my inbox

  Rule: Only a user's own action sets notifications aside

    @legacy:test_set_aside_notifications::test_failing_connection_keeps_notifications_visible
    Scenario: A connection the API moved to failing keeps its notifications visible
      Given I have a GitHub notification that I read, snoozed and turned into a task
      And my GitHub connection turned failing, as the API does when a refresh token expires
      When I switch the notifications of my GitHub connection back on
      Then my notification is unchanged in my inbox
