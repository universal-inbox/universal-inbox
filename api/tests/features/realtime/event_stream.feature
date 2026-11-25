Feature: The realtime event stream
  A signed-in user keeps one server-sent event stream open per tab. It carries
  only their own events, whatever process publishes them, tells them when it is
  alive, and ends when their session does. The stream is not for API tokens, is
  capped per user, and does not exist when the app has realtime events disabled.
  A pod also schedules the syncs of the users who keep a stream open.

  Rule: The stream delivers the events of its user

    @legacy:test_realtime_stream::test_event_stream_headers
    Scenario: The stream is served uncompressed and unbuffered, as a stream of events
      Given I am signed in
      When I request the event stream, accepting the encodings "gzip, br"
      Then the app answers 200
      And the response header "content-type" is "text/event-stream"
      And the response header "cache-control" is "no-cache"
      And the response header "x-accel-buffering" is "no"
      And the response header "content-encoding" is "identity"

    @legacy:test_realtime_stream::test_event_stream_delivers_the_events_of_the_user_only
    Scenario: A change of a notification reaches the stream of its owner and not the stream of another user
      Given I am signed in
      And I am connected to GitHub
      And I have a GitHub notification called "mine"
      And I have the event stream open
      And another user is signed in
      And that other user has the event stream open
      When I snooze "mine" until "2022-01-01T01:02:03Z"
      Then my stream delivers the event notifications.changed, for "mine" only
      When a worker publishes the event tasks.changed for that other user
      Then the stream of that other user delivers the event tasks.changed, with no ids
      When a worker publishes the event integration_connection.changed for me
      Then my stream delivers the event integration_connection.changed, with no ids

    @legacy:test_realtime_stream::test_event_stream_delivers_events_published_by_another_process
    Scenario: An event published by a worker reaches the stream of its user
      Given I am signed in
      And I have the event stream open
      When a worker publishes the event integration_connection.changed for me
      Then my stream delivers the event integration_connection.changed, with no ids

    @legacy:test_realtime_stream::test_event_stream_sends_heartbeats
    Scenario: The stream sends heartbeats while nothing happens
      Given I am signed in
      And I have the event stream open
      Then my stream sends two heartbeats in a row

    @legacy:test_realtime_stream::test_event_stream_ends_when_the_session_is_revoked
    Scenario: The stream ends when the session of its user is revoked
      Given I am signed in
      And I have the event stream open
      When I sign out
      Then my stream ends

  Rule: Who may open a stream

    Scenario: A visitor without any credential is refused
      When a visitor without any credential requests the event stream
      Then the app answers 401

    @legacy:test_realtime_stream::test_event_stream_rejects_api_keys
    Scenario: An API token is refused
      Given I am signed in
      And I have an API token
      When I request the event stream with my API token, instead of my session
      Then the app answers 403

    @legacy:test_realtime_stream::test_event_stream_is_not_found_when_realtime_is_disabled
    Scenario: The stream does not exist when realtime events are disabled
      Given the app does not serve realtime events
      And I am signed in
      When I request the event stream
      Then the app answers 404

    @legacy:test_realtime_stream::test_event_stream_is_refused_above_the_streams_cap
    Scenario: A user above the cap of open streams is refused
      Given I am signed in
      Given another user is signed in
      When I open as many event streams as a user may
      And I request the event stream
      Then the app answers 429
      And that other user can still open an event stream

  Rule: The syncs of the users with an open stream are scheduled

    Background:
      Given the app schedules the syncs of the users with an open stream every second
      And I am signed in
      And I am connected to GitHub
      And my GitHub connection was never scheduled for a sync

    @legacy:test_realtime_stream::test_due_syncs_of_a_connected_user_are_scheduled
    Scenario: The due syncs of a user with an open stream are scheduled and announced on the stream
      When I open the event stream
      Then my stream delivers the event integration_connection.changed, for my GitHub connection only
      And my GitHub connection was scheduled for a sync

    @legacy:test_realtime_stream::test_due_syncs_of_a_disconnected_user_are_not_scheduled
    Scenario: The syncs of a user without an open stream are not scheduled
      When I wait two seconds, as long as two scheduling intervals
      Then my GitHub connection was never scheduled for a sync
