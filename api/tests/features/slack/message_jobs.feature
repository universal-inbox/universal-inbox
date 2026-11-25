Feature: Handling a Slack message
  The job that handles a Slack message decides, for each user whose Slack connection can see it,
  whether to create, update or leave their notification of the thread.

  Background:
    Given I am signed in

  Rule: A message creates a notification for the users it mentions

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_channel_without_ping
    Scenario: A message that mentions no one creates nothing
      Given Slack sends a message in a channel
      When the Slack message job handles that event
      Then the job makes no decision
      When I look at my notifications
      Then nobody has any notification

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_channel_with_disabled_sync
    Scenario: A mentioned user whose Slack sync is disabled gets nothing
      Given I am connected to Slack as the Slack user "U01", with its sync disabled
      And Slack sends a message in a channel
      And that message mentions the Slack user "U01"
      When the Slack message job handles that event
      Then the job decides sync_disabled
      When I look at my notifications
      Then nobody has any notification

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_channel_with_ping_to_unknown_user
    Scenario: A message that mentions a user who has no Slack connection creates nothing
      Given I am connected to Slack as the Slack user "U01"
      And Slack sends a message in a channel
      And that message mentions the Slack user "U02"
      When the Slack message job handles that event
      Then the job makes no decision
      When I look at my notifications
      Then nobody has any notification

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_channel_with_ping_to_known_user::case_1_user_ping_and_embedded_user_profiles
    Scenario: A message that mentions a user creates their notification, and a retry of Slack changes nothing
      Given I am connected to Slack as the Slack user "U01"
      And Slack sends a message in a channel
      And that message mentions the Slack user "U01"
      And Slack serves the permalink of that message
      And Slack serves a thread with embedded user profiles of that message to the token "slack_test_user_access_token"
      And Slack serves the channel "C05XXX"
      And Slack serves the team "T01"
      When the Slack message job handles that event, then handles it again as Slack retries events it did not see acknowledged
      Then the job decides created
      And the second time, the job decides unchanged
      When I look at my notifications
      Then that is the only notification: the Slack thread "1732535291.911209", titled "Hello", unread
      And that notification is unread, with no read date, no task and no snooze
      And that Slack thread is in the channel "C05XXX"
      And that Slack thread has the sender profiles of "U01" and "U02"
      And that Slack thread is subscribed

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_channel_with_ping_to_known_user::case_2_usergroup_ping
    Scenario: A message that mentions a usergroup creates the notification of its members, and a retry of Slack changes nothing
      Given I am connected to Slack as the Slack user "U01"
      And Slack sends a message in a channel
      And that message mentions the Slack usergroup "G01"
      And Slack serves the members of the usergroup "G01"
      And Slack serves the permalink of that message
      And Slack serves the Slack user "U01"
      And Slack serves the Slack user "U02"
      And Slack serves an ordinary thread of that message to the token "slack_test_user_access_token"
      And Slack serves the channel "C05XXX"
      And Slack serves the team "T01"
      When the Slack message job handles that event, then handles it again as Slack retries events it did not see acknowledged
      Then the job decides created
      And the second time, the job decides unchanged
      When I look at my notifications
      Then that is the only notification: the Slack thread "1732535291.911209", titled "Hello", unread
      And that notification is unread, with no read date, no task and no snooze
      And that Slack thread is in the channel "C05XXX"
      And that Slack thread has the sender profiles of "U01" and "U02"
      And that Slack thread is subscribed

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_channel_with_ping_to_known_user_from_known_user
    Scenario: A usergroup mention creates the notification of a member, and reads the one of the sender
      Given another user is signed in
      And the other user is connected to Slack as the Slack user "U02", with the access token "slack_other_user_access_token"
      And I am connected to Slack as the Slack user "U01"
      And Slack sends a message in a channel
      And that message mentions the Slack usergroup "G01"
      And Slack serves the members of the usergroup "G01"
      And Slack serves the Slack user "U01"
      And Slack serves the Slack user "U02"
      And Slack serves an ordinary thread of that message to the token "slack_test_user_access_token"
      And Slack serves a subscribed thread, read up to its second message of that message to the token "slack_other_user_access_token"
      And Slack serves the permalink of that message
      And Slack serves the permalink of the message "1729779674.478289"
      And Slack serves its emojis
      And Slack serves the channel "C05XXX"
      And Slack serves the team "T01"
      When the Slack message job handles that event
      Then the job decides created, deleted_last_from_user
      When the other user looks at their notifications
      Then that is the only notification: the Slack thread "1732535291.911209", deleted
      When I look at my notifications
      Then that is the only notification: the Slack thread "1732535291.911209", titled "Hello", unread
      And that notification is unread, with no read date, no task and no snooze
      And that Slack thread is in the channel "C05XXX"
      And that Slack thread has the sender profiles of "U01" and "U02"
      And that Slack thread is subscribed
      And that Slack thread names the emoji "unknown1" as an alias of "wave"

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_edited_to_add_a_ping_to_known_user
    Scenario: An edit that adds a mention creates the notification
      Given I am connected to Slack as the Slack user "U01"
      And Slack sends the edit of a message that mentions the Slack user "U01"
      And Slack serves the permalink of that message
      And Slack serves the Slack user "U01"
      And Slack serves the Slack user "U02"
      And Slack serves an ordinary thread of that message to the token "slack_test_user_access_token"
      And Slack serves the channel "C05XXX"
      And Slack serves the team "T01"
      When the Slack message job handles that event
      Then the job decides created
      When I look at my notifications
      Then that is the only notification: the Slack thread "1732535291.911209", unread
      And that Slack thread is in the channel "C05XXX"

  Rule: Slack answers that it cannot resolve something do not stop the job

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_channel_with_unknown_bot_sender
    Scenario: A bot that Slack does not know is skipped, and the notification is still created
      Given I am connected to Slack as the Slack user "U01"
      And Slack sends a message in a channel
      And that message mentions the Slack user "U01"
      And Slack serves the permalink of that message
      And Slack serves the Slack user "U01"
      And Slack does not know the bot "B05YYY"
      And Slack serves a thread with a bot of that message to the token "slack_test_user_access_token"
      And Slack serves the channel "C05XXX"
      And Slack serves the team "T01"
      When the Slack message job handles that event
      Then the job decides created
      When I look at my notifications
      Then that is the only notification: the Slack thread "1732535291.911209", titled "Hello", unread
      And that Slack thread has the sender profile of "U01", but not the one of "B05YYY"

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_channel_with_failing_bot_fetch
    Scenario: Any other error of Slack, such as rate limiting, fails the job so that it is retried
      Given I am connected to Slack as the Slack user "U01"
      And Slack sends a message in a channel
      And that message mentions the Slack user "U01"
      And Slack serves the permalink of that message
      And Slack serves the Slack user "U01"
      And Slack is rate limiting the requests about the bot "B05YYY"
      And Slack serves a thread with a bot of that message to the token "slack_test_user_access_token"
      And Slack serves the channel "C05XXX"
      And Slack serves the team "T01"
      When the Slack message job handles that event
      Then the job fails, with an error mentioning "ratelimited"

    @legacy:test_slack_webhook_message::job::test_unresolvable_slack_bot_and_user_are_negatively_cached
    Scenario: A bot and a user that Slack does not know are asked about once, not on every fetch
      Given Slack does not know the bot "B05UNKNOWN" nor the user "U05UNKNOWN", and is asked once about each
      When the app asks Slack twice who the bot "B05UNKNOWN" and the user "U05UNKNOWN" are, with the token "slack_test_user_access_token"
      Then Slack does not know either of them, both times

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_channel_with_ping_to_unknown_usergroup
    Scenario: A mention of a usergroup that Slack does not know notifies no one
      Given I am connected to Slack as the Slack user "U01"
      And Slack sends a message in a channel
      And that message mentions the Slack usergroup "S06UNKNOWN"
      And Slack does not know the usergroup "S06UNKNOWN"
      When the Slack message job handles that event
      Then the job makes no decision
      When I look at my notifications
      Then nobody has any notification

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_channel_with_unknown_team
    Scenario: A team that Slack does not know is kept as its id only
      Given I am connected to Slack as the Slack user "U01"
      And Slack sends a message in a channel
      And that message mentions the Slack user "U01"
      And Slack serves the permalink of that message
      And Slack serves the Slack user "U01"
      And Slack serves the Slack user "U02"
      And Slack serves an ordinary thread of that message to the token "slack_test_user_access_token"
      And Slack serves the channel "C05XXX"
      And Slack does not know the team "T01"
      When the Slack message job handles that event
      Then the job decides created
      When I look at my notifications
      Then that is the only notification: the Slack thread "1732535291.911209", unread
      And that Slack thread only knows the team "T01"

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_dm_not_visible_to_pinged_user
    Scenario: A conversation that the mentioned user cannot see is skipped without failing the job
      Given I am connected to Slack as the Slack user "U01"
      And Slack sends a message in a channel
      And that message mentions the Slack user "U01"
      And Slack does not let the token "slack_test_user_access_token" see the thread of that message, and is asked about it once
      When the Slack message job handles that event
      Then the job decides thread_not_accessible
      When I look at my notifications
      Then nobody has any notification

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_thread_referencing_unknown_usergroup
    Scenario: A usergroup that Slack does not list does not prevent the other references from being resolved
      Given I am connected to Slack as the Slack user "U01"
      And Slack sends a message in a channel
      And that message mentions the Slack user "U01"
      And Slack serves the permalink of that message
      And Slack serves the Slack user "U01"
      And Slack serves the Slack user "U02"
      And Slack serves a thread that references an unknown usergroup of that message to the token "slack_test_user_access_token"
      And Slack serves the channel "C05XXX"
      And Slack serves the team "T01"
      And Slack lists its usergroups
      And Slack serves the members of the usergroup "S05ZZZ"
      And Slack does not know the usergroup "S06UNKNOWN"
      And Slack serves its emojis
      When the Slack message job handles that event
      Then the job decides created
      When I look at my notifications
      Then that is the only notification: the Slack thread "1732535291.911209", unread
      And that Slack thread names the usergroup "S05ZZZ" "admins"
      And that Slack thread could not name the usergroup "S06UNKNOWN"
      And that Slack thread highlights only the usergroup "S05ZZZ"

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_thread_with_plan_and_unknown_blocks
    Scenario: A thread with a block that the Slack library does not model is still stored whole
      Given I am connected to Slack as the Slack user "U01"
      And Slack sends a message in a channel
      And that message mentions the Slack user "U01"
      And Slack serves the permalink of that message
      And Slack serves the Slack user "U01"
      And Slack serves the Slack user "U02"
      And Slack serves a thread with a plan block and an unknown block of that message to the token "slack_test_user_access_token"
      And Slack serves the channel "C05XXX"
      And Slack serves the team "T01"
      And Slack serves its emojis
      When the Slack message job handles that event
      Then the job decides created
      When I look at my notifications
      Then that is the only notification: the Slack thread "1732535291.911209", unread
      And the last message of that Slack thread keeps its plan block and its unknown block

  Rule: A reply in a thread that the user follows updates their notification

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_unknown_thread_without_ping
    Scenario: A reply in a thread that no one follows creates nothing
      Given Slack sends a message in a thread
      When the Slack message job handles that event
      Then the job makes no decision
      When I look at my notifications
      Then nobody has any notification

    @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread_from_known_user
    Scenario: A reply updates the notification of the reader, and reads the one of its sender
      Given another user is signed in
      And the other user is connected to Slack as the Slack user "U02", with the access token "slack_other_user_access_token"
      And Slack serves the permalink of the first message of the thread
      And I am connected to Slack as the Slack user "U01"
      And Slack serves the permalink of the last message of the thread
      And I know a Slack thread that is subscribed, read up to its last message
      And the other user knows a Slack thread that is subscribed and not read yet
      And Slack sends a message in a thread
      And that message is a reply in the known thread
      And Slack serves the Slack user "U01"
      And Slack serves the Slack user "U02"
      And Slack serves a subscribed thread, read up to its second message of that message to the token "slack_other_user_access_token"
      And Slack serves an ordinary thread of that message to the token "slack_test_user_access_token"
      And Slack serves the channel "C05XXX"
      And Slack serves the team "T01"
      When the Slack message job handles that event
      Then the job decides deleted_last_from_user, updated
      When the other user looks at their notifications
      Then that is the only notification: the Slack thread "1732535291.911209", deleted
      When I look at my notifications
      Then that is the only notification: the Slack thread "1732535291.911209", unread

    Scenario Outline: With two-way sync, the notification follows the subscription of the thread in Slack
      Given I am connected to Slack, with two-way sync
      And Slack serves the permalink of the last message of the thread
      And I know a Slack thread that is <was>, read up to its last message
      And Slack sends a message in a thread
      And that message is a reply in the known thread
      And Slack serves the Slack user "U01"
      And Slack serves the Slack user "U02"
      And Slack serves <slack_thread> of that message to the token "slack_test_user_access_token"
      And Slack serves the channel "C05XXX"
      And Slack serves the team "T01"
      When the Slack message job handles that event
      Then the job decides <decision>
      When I look at my notifications
      Then that is the only notification: the Slack thread "1732535291.911209", titled "World", <status>
      And that Slack thread is <subscribed_after>

      @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread::case_01_is_2way_sync_was_subscribed_is_not_subscribed
      Examples:
        | was        | slack_thread                                          | decision      | status       | subscribed_after |
        | subscribed | an unsubscribed thread, read up to its first message  | unsubscribed  | unsubscribed | unsubscribed     |

      @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread::case_02_is_2way_sync_was_subscribed_is_subscribed
      Examples:
        | was        | slack_thread                                       | decision | status | subscribed_after |
        | subscribed | a subscribed thread, read up to its first message  | updated  | unread | subscribed       |

      @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread::case_03_is_2way_sync_was_not_subscribed_is_not_subscribed
      Examples:
        | was          | slack_thread                                          | decision  | status       | subscribed_after |
        | unsubscribed | an unsubscribed thread, read up to its first message  | unchanged | unsubscribed | unsubscribed     |

      @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread::case_04_is_2way_sync_was_not_subscribed_is_subscribed
      Examples:
        | was          | slack_thread                                       | decision | status | subscribed_after |
        | unsubscribed | a subscribed thread, read up to its first message  | updated  | unread | subscribed       |

    Scenario Outline: With one-way sync, a mention of the user brings the notification back when the thread is subscribed
      Given I am connected to Slack as the Slack user "U01", with one-way sync
      And Slack serves the permalink of the last message of the thread
      And I know a Slack thread that is <was>, read up to its last message
      And Slack sends a message in a thread
      And that message mentions the Slack user "U01"
      And that message is a reply in the known thread
      And Slack serves the Slack user "U01"
      And Slack serves the Slack user "U02"
      And Slack serves <slack_thread> of that message to the token "slack_test_user_access_token"
      And Slack serves the channel "C05XXX"
      And Slack serves the team "T01"
      When the Slack message job handles that event
      Then the job decides <decision>
      When I look at my notifications
      Then that is the only notification: the Slack thread "1732535291.911209", titled "World", <status>
      And that Slack thread is <subscribed_after>

      @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread::case_05_is_1way_sync_was_subscribed_is_not_subscribed_pinged
      Examples:
        | was        | slack_thread                                          | decision     | status       | subscribed_after |
        | subscribed | an unsubscribed thread, read up to its first message  | unsubscribed | unsubscribed | unsubscribed     |

      @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread::case_06_is_1way_sync_was_subscribed_is_subscribed_pinged
      Examples:
        | was        | slack_thread                                       | decision | status | subscribed_after |
        | subscribed | a subscribed thread, read up to its first message  | updated  | unread | subscribed       |

      @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread::case_07_is_1way_sync_was_not_subscribed_is_not_subscribed_pinged
      Examples:
        | was          | slack_thread                                          | decision  | status       | subscribed_after |
        | unsubscribed | an unsubscribed thread, read up to its first message  | unchanged | unsubscribed | unsubscribed     |

      @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread::case_08_is_1way_sync_was_not_subscribed_is_subscribed_pinged
      Examples:
        | was          | slack_thread                                       | decision | status | subscribed_after |
        | unsubscribed | a subscribed thread, read up to its first message  | updated  | unread | subscribed       |

    Scenario Outline: With one-way sync, a reply that mentions no one only keeps the notification of a thread that was followed
      Given I am connected to Slack, with one-way sync
      And Slack serves the permalink of the last message of the thread
      And I know a Slack thread that is <was>, read up to its last message
      And Slack sends a message in a thread
      And that message is a reply in the known thread
      And Slack serves the Slack user "U01"
      And Slack serves the Slack user "U02"
      And Slack serves <slack_thread> of that message to the token "slack_test_user_access_token"
      And Slack serves the channel "C05XXX"
      And Slack serves the team "T01"
      When the Slack message job handles that event
      Then the job decides <decision>
      When I look at my notifications
      Then that is the only notification: the Slack thread "1732535291.911209", titled "World", <status>
      And that Slack thread is <subscribed_after>

      @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread::case_09_is_1way_sync_was_subscribed_is_not_subscribed_not_pinged
      Examples:
        | was        | slack_thread                                          | decision     | status       | subscribed_after |
        | subscribed | an unsubscribed thread, read up to its first message  | unsubscribed | unsubscribed | unsubscribed     |

      @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread::case_10_is_1way_sync_was_subscribed_is_subscribed_not_pinged
      Examples:
        | was        | slack_thread                                       | decision | status | subscribed_after |
        | subscribed | a subscribed thread, read up to its first message  | updated  | unread | subscribed       |

      @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread::case_11_is_1way_sync_was_not_subscribed_is_not_subscribed_not_pinged
      Examples:
        | was          | slack_thread                                          | decision  | status       | subscribed_after |
        | unsubscribed | an unsubscribed thread, read up to its first message  | unchanged | unsubscribed | unsubscribed     |

      @legacy:test_slack_webhook_message::job::test_handle_slack_message_in_known_thread::case_12_is_1way_sync_was_not_subscribed_is_subscribed_not_pinged
      Examples:
        | was          | slack_thread                                       | decision  | status       | subscribed_after |
        | unsubscribed | a subscribed thread, read up to its first message  | unchanged | unsubscribed | unsubscribed     |
