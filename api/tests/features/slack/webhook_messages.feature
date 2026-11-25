Feature: Receiving Slack messages
  Slack tells the app about messages. The app only queues the ones that can matter to a user:
  a message that mentions someone, or that is in a thread the user already follows.

  Background:
    Given I am signed in

  Rule: A new message is queued when it mentions someone or is in a known thread

    @legacy:test_slack_webhook_message::webhook::test_receive_slack_message_in_channel_without_ping_to_known_users
    Scenario: A message in a channel that mentions no one is ignored
      Given Slack sends a message in a channel
      When Slack delivers that event to the app
      Then the app answers 200
      And the event is ignored

    @legacy:test_slack_webhook_message::webhook::test_receive_slack_message_in_channel_with_references
    Scenario: A message in a channel that mentions someone is queued
      Given Slack sends a message in a channel
      And that message mentions the Slack user "user1"
      When Slack delivers that event to the app
      Then the app answers 200
      And the event is queued for processing

    @legacy:test_slack_webhook_message::webhook::test_receive_slack_message_in_an_unknown_thread
    Scenario: A message in a thread that no one follows is ignored
      Given Slack sends a message in a thread
      When Slack delivers that event to the app
      Then the app answers 200
      And the event is ignored

    Scenario Outline: A message in a thread of the user is queued, even if the user unsubscribed, as the user may have subscribed again from Slack
      Given I am connected to Slack, syncing its notifications
      And I know a Slack thread that I am <subscription> to
      And Slack sends a message in a thread
      And that message is a reply in the known thread
      When Slack delivers that event to the app
      Then the app answers 200
      And the event is queued for processing

      @legacy:test_slack_webhook_message::webhook::test_receive_slack_message_in_known_thread::case_1_subscribed
      Examples:
        | subscription |
        | subscribed   |

      @legacy:test_slack_webhook_message::webhook::test_receive_slack_message_in_known_thread::case_2_unsubscribed
      Examples:
        | subscription |
        | unsubscribed |

    @legacy:test_slack_webhook_message::webhook::test_receive_slack_message_in_known_thread_from_known_user
    Scenario: A message in a known thread, sent by another Slack user than the one of the connection, is queued
      Given I am connected to Slack, syncing its notifications, as the Slack user "U02"
      And I know a Slack thread that I am subscribed to
      And Slack sends a message in a thread
      And that message is a reply in the known thread
      When Slack delivers that event to the app
      Then the app answers 200
      And the event is queued for processing

  Rule: The edit of a message is queued when it now mentions someone or is in a known thread

    @legacy:test_slack_webhook_message::webhook::test_receive_slack_message_edited_to_add_a_reference
    Scenario: An edit that adds a mention is queued
      Given Slack sends the edit of a message that mentions the Slack user "U01"
      When Slack delivers that event to the app
      Then the app answers 200
      And the event is queued for processing

    @legacy:test_slack_webhook_message::webhook::test_receive_slack_message_edited_without_reference_in_an_unknown_thread
    Scenario: An edit that mentions no one, in a thread that no one follows, is ignored
      Given Slack sends the edit of a message
      And that edit mentions no one anymore
      When Slack delivers that event to the app
      Then the app answers 200
      And the event is ignored

    @legacy:test_slack_webhook_message::webhook::test_receive_slack_message_edited_without_reference_in_known_thread
    Scenario: An edit that mentions no one, in a known thread, is queued
      Given I am connected to Slack, syncing its notifications
      And I know a Slack thread that I am subscribed to
      And Slack sends the edit of a message
      And that edit mentions no one anymore
      And that edit is in the known thread
      When Slack delivers that event to the app
      Then the app answers 200
      And the event is queued for processing

    @legacy:test_slack_webhook_message::webhook::test_receive_slack_thread_reply_edited_to_add_a_reference
    Scenario: The edit of a reply is queued as that reply, in its own thread
      Given Slack sends the edit of a message that mentions the Slack user "U01"
      And that edit is the one of the reply "1732535350.000100" in the thread "1732535291.911209"
      When Slack delivers that event to the app
      Then the app answers 200
      And the queued job is the message "1732535350.000100" of the thread "1732535291.911209", mentioning the Slack user "U01"

    @legacy:test_slack_webhook_message::webhook::test_receive_slack_message_changed_without_content_change
    Scenario: An edit that changes no content, such as a link preview, does not sync the thread again
      Given Slack sends the edit of a message
      And that edit only adds a link preview to the message
      When Slack delivers that event to the app
      Then the app answers 200
      And the event is ignored

    @legacy:test_slack_webhook_message::webhook::test_receive_slack_message_edited_with_invalid_thread_ts
    Scenario: An edit in a thread whose timestamp cannot be read is ignored, not taken as a message outside of any thread
      Given Slack sends the edit of a message
      And that edit is in a thread whose timestamp is the number 42
      When Slack delivers that event to the app
      Then the app answers 200
      And the event is ignored
