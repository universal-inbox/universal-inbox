Feature: Security headers and cross-origin requests
  Every response carries the headers that protect the web app from clickjacking
  and content sniffing. API responses are never cached nor rendered by the
  browser, and other websites cannot use a visitor's session.

  Rule: Pages and other responses declare their security policy

    @legacy:test_misc::content_security_policy::test_csp_header_on_html_page
    Scenario: The web app page restricts what it can load
      The policy allows the app's own origin, its identity provider and a few
      third-party widgets, and the inline bootstrap script of the page.

      When I request "/"
      Then the app answers 200
      And the response header "content-type" is "text/html; charset=utf-8"
      And the response header "content-security-policy" is:
        """
        default-src 'self'; script-src 'self' 'wasm-unsafe-eval' https://client.crisp.chat https://cdn.headwayapp.co 'sha256-uAESwGgY2G0W8BhcAjQ5tDZK88YZcbjq65DW8JTcims='; style-src 'self' 'unsafe-inline' https://client.crisp.chat; object-src 'none'; connect-src 'self' <identity provider URL> https://client.crisp.chat wss://client.relay.crisp.chat; img-src * 'self' data:; font-src 'self' https://client.crisp.chat; worker-src 'none'; frame-src 'self' https://headway-widget.net; frame-ancestors 'self'; base-uri 'self'; form-action 'self'
        """
      And the response header "x-frame-options" is "DENY"
      And the response header "strict-transport-security" is "max-age=63072000; includeSubDomains; preload"
      And the response carries the common security headers
      And the response header "content-disposition" is not "attachment"

    @legacy:test_misc::content_security_policy::test_csp_header_on_other_url
    Scenario: A JSON response has no content security policy but keeps the other headers
      The policy only constrains pages, but clickjacking and HTTPS protections
      apply to every response.

      When I request "/ping"
      Then the app answers 200
      And the response header "content-type" is "application/json"
      And the response has no "content-security-policy" header
      And the response header "x-frame-options" is "DENY"
      And the response header "strict-transport-security" is "max-age=63072000; includeSubDomains; preload"
      And the response carries the common security headers
      And the response has no "content-disposition" header
      And the response header "cache-control" is not "no-store"

  Rule: API responses are not cached nor rendered by the browser

    @legacy:test_misc::api_response_headers::test_authenticated_api_response_is_not_cached_nor_rendered
    Scenario: A signed-in user's API response is not cached nor rendered
      Given I am signed in
      When I request "/api/notifications"
      Then the app answers 200
      And the response header "cache-control" is "no-store"
      And the response header "content-disposition" is "attachment"
      And the response carries the common security headers

    @legacy:test_misc::api_response_headers::test_api_error_response_is_not_cached_nor_rendered
    Scenario: An API error response is not cached nor rendered
      When I request "/api/notifications"
      Then the app answers 401
      And the response header "cache-control" is "no-store"
      And the response header "content-disposition" is "attachment"
      And the response carries the common security headers

    @legacy:test_misc::api_response_headers::test_explicit_cache_control_is_kept
    Scenario: A cache policy chosen by an API route is kept
      Given I am signed in
      When I request "/api/front_config"
      Then the app answers 200
      And the response header "cache-control" is "private, max-age=5"
      When I request "/api/tasks/search"
      Then the app answers 200
      And the response header "cache-control" is "private, max-age=600"
      And the response header "content-disposition" is "attachment"

  Rule: Other websites cannot use a visitor's session

    @legacy:test_misc::cors::test_cors_rejects_arbitrary_origin_on_cookie_authed_endpoint
    Scenario: An arbitrary website is not allowed to call a cookie-authenticated endpoint
      Browsers ask before sending a cross-origin request. The answer must not
      echo the website's origin, nor combine any origin with credentials.

      When I send an OPTIONS request to "/api/notifications" with the headers:
        | Origin                         | https://evil.example |
        | Access-Control-Request-Method  | GET                  |
        | Access-Control-Request-Headers | authorization        |
      Then the response does not allow the origin "https://evil.example"
      And the response does not combine a wildcard origin with credentials

    @legacy:test_misc::cors::test_cors_mcp_endpoint_does_not_credential_arbitrary_origin
    Scenario: An arbitrary website cannot use credentials with the MCP endpoint
      The MCP endpoint only accepts bearer tokens, so no website origin may be
      allowed to send credentials to it.

      When I send an OPTIONS request to "/api/mcp/" with the headers:
        | Origin                         | https://evil.example |
        | Access-Control-Request-Method  | POST                 |
        | Access-Control-Request-Headers | authorization        |
      Then a response that allows an origin does not allow credentials
