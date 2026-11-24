Feature: User activity
  The app records when a user was last active, which decides when their
  connections are paused for inactivity. A signed-in request records it, at
  most once a day.

  @legacy:test_pause_integration_connections::test_authenticated_requests_record_user_activity_once_a_day
  Scenario: A signed-in request records the activity, once a day
    Given the app lets users sign in with a password or a passkey
    And I have an account with a password
    And I signed in with my password
    And I was last active 30 days ago
    When I open my account page
    Then my last activity is recorded as just now
    When I was last active 30 days ago
    When I open my account page
    Then my last activity is still the one recorded before
