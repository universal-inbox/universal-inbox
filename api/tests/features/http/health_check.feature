Feature: Service health check
  Uptime monitors and smoke tests call the unauthenticated health check to know
  whether the service can reach its database and its cache.

  Scenario: The health check reports a healthy database and cache
    When I check the service health
    Then the health check succeeds
    And the health check answers exactly:
      """
      {"cache":"healthy","database":"healthy"}
      """

  Scenario: The health check is rate limited per IP address
    The budget is 60 checks per minute and per IP address.

    When I keep checking the service health, up to 65 times, until it is refused
    Then a health check is refused for too many requests
