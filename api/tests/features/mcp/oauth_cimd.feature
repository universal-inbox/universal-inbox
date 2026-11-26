Feature: Authorizing an OAuth2 client identified by a URL
  A client can use the URL of its metadata document as its client_id (Client ID Metadata
  Discovery). The app reads the document from its cache, enforces the redirect URIs it
  declares like those of a registered client, and fetches it again once the cache expired.
  The tests seed the cache, so that no outbound fetch is made.

  Background:
    Given I am signed in
    And I have an API token

  Scenario: A client identified by a URL is accepted from the cache and sent to the consent screen
    Given an OAuth2 client identified by a URL, whose cached metadata document declares the redirect URI "http://localhost:12345/callback" for the next 24 hours
    And my PKCE verifier is "verifier-cimd"
    When I ask to authorize that client
    Then the app answers 302
    And the app redirects to the consent screen of the front end, for a new request and without a code

  Scenario: A redirect URI that the metadata document does not declare is refused
    Given an OAuth2 client identified by a URL, whose cached metadata document declares the redirect URI "https://cimd.example.test/cb" for the next 24 hours
    And my PKCE verifier is "verifier-cimd"
    When I ask to authorize that client
    Then the app answers 400
    And the error message is "Invalid input data: Invalid redirect_uri: http://localhost:12345/callback"

  Scenario: An expired metadata document is not served stale: its fetch, which cannot succeed here, fails the request
    Given an OAuth2 client identified by a URL, whose cached metadata document declares the redirect URI "http://localhost:12345/callback" and expired an hour ago
    And my PKCE verifier is "verifier-cimd"
    When I ask to authorize that client
    Then the app answers 400
    And the response has no "location" header
    And the error message mentions "Failed to resolve CIMD host cimd.example.test"

  Scenario: The authorization server metadata advertises the support of metadata documents
    When I request "/.well-known/oauth-authorization-server"
    Then the app answers 200
    And the response field "client_id_metadata_document_supported" is:
      """
      true
      """

  Scenario: A metadata document on this machine is refused without being fetched, when its URL names an IP address
    Given a local server, where the app is not told to fetch the metadata documents
    And the local server is not asked anything
    And an OAuth2 client identified by the HTTPS URL of a metadata document on that local server, named "127.0.0.1"
    And my PKCE verifier is "verifier-cimd"
    When I ask to authorize that client
    Then the app answers 400
    And the response has no "location" header
    And the error message is "Invalid input data: CIMD URL host must be a domain name, got IP literal 127.0.0.1"
    And the local server was asked exactly what it expected

  Scenario: A metadata document on this machine is refused without being fetched, when its URL names a host that resolves to this machine
    Given a local server, where the app is not told to fetch the metadata documents
    And the local server is not asked anything
    And an OAuth2 client identified by the HTTPS URL of a metadata document on that local server, named "localhost"
    And my PKCE verifier is "verifier-cimd"
    When I ask to authorize that client
    Then the app answers 400
    And the response has no "location" header
    And the error message mentions "CIMD host localhost resolves to a special-use address ("
    And the local server was asked exactly what it expected
