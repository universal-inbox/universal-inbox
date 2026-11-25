Feature: Receiving the events of Slack
  Slack signs every event it sends. The app answers the URL verification that Slack does
  when the webhook is set up, tells Slack that it received the events it does not handle
  without queuing anything, and refuses an event that carries no signature.

  @legacy:test_slack_webhook::test_receive_slack_url_verification_event
  Scenario: The URL verification of Slack gets its challenge back
    Given Slack sends a URL verification with the challenge "test challenge"
    When Slack delivers that event to the app
    Then the app answers 200
    And Slack gets the challenge "test challenge" back, and nothing else
    And the event is ignored

  @legacy:test_slack_webhook::test_slack_webhook_rejects_unsigned_request
  Scenario: A URL verification without a signature is refused, and its challenge is not given back
    Given Slack sends a URL verification with the challenge "test challenge"
    When that event reaches the app without a signature
    Then the app answers 401
    And the answer does not give the challenge "test challenge" back

  @legacy:test_slack_webhook::test_receive_slack_ignored_event
  Scenario: An event that is not an event callback is acknowledged and not queued
    Given Slack sends an event telling that the app is rate limited
    When Slack delivers that event to the app
    Then the app answers 200
    And the event is ignored

  @legacy:test_slack_webhook::test_receive_slack_ignored_push_event_callback
  Scenario: An event callback that the app does not handle is acknowledged and not queued
    Given Slack sends an event telling that a user opened the home of the app
    When Slack delivers that event to the app
    Then the app answers 200
    And the event is ignored
