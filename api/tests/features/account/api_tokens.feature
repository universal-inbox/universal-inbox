Feature: API tokens
  A signed-in user creates API tokens to call the API from scripts. The app
  stores only a digest of a token, a revoked token or one whose stored expiry
  passed stops working, and nobody can revoke a token that is not theirs.

  Background:
    Given I am signed in

  Scenario: A user creates an API token
    When I create an API token
    Then the API token is mine, is not a session token and is neither revoked nor expired
    And my account has that one API token, which remembers only the last 5 characters of it
    And the app stores only a digest of the API token, never the token itself

  Scenario: A revoked API token stops working
    Given I have an API token
    Then the API token is accepted as a bearer token
    When I revoke my API token
    Then the app answers 204
    And the API token is rejected as a bearer token
    And my account marks the API token as revoked

  Scenario: An API token whose stored expiry passed stops working
    Given I have an API token
    When the stored expiry of my API token passes while the token itself is still valid
    Then the API token is rejected as a bearer token

  Scenario: A user cannot revoke an API token that is not theirs
    When I revoke an API token that I never created
    Then the app answers 400
