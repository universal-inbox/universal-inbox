Feature: Limiting requests per network address
  The sign-in, registration, password reset and passkey endpoints accept a
  limited number of requests per minute from one network address, so a single
  address cannot brute force passwords or flood the app.

  Background:
    Given the app lets users sign in with a password or a passkey

  Scenario: A network address that sent too many requests is throttled, another one is not
    When 35 sign-in attempts with a wrong password for an email address that no account uses arrive at the same time from one network address
    Then some of those attempts were throttled
    When a sign-in attempt with a wrong password for another email address arrives from another network address
    Then the app does not throttle it
