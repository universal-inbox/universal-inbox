Feature: Authorizing an OAuth2 client identified by a URL
  A client can use the URL of its metadata document as its client_id (Client ID Metadata
  Discovery). The app reads the document from its cache, enforces the redirect URIs it
  declares like those of a registered client, and fetches it again once the cache expired.
  The tests seed the cache, so that no outbound fetch is made.

  Background:
    Given I am signed in
    And I have an API token

  @legacy:test_oauth2_cimd::cimd_authorize_resolves_client_from_cache
  Scenario: A client identified by a URL is accepted from the cache and sent to the consent screen
    Given an OAuth2 client identified by a URL, whose cached metadata document declares the redirect URI "http://localhost:12345/callback" for the next 24 hours
    And my PKCE verifier is "verifier-cimd"
    When I ask to authorize that client
    Then the app answers 302
    And the response header "location" contains "/oauth2/consent?request_id="
    And the response header "location" does not contain "code="

  @legacy:test_oauth2_cimd::cimd_authorize_rejects_redirect_uri_not_in_doc
  Scenario: A redirect URI that the metadata document does not declare is refused
    Given an OAuth2 client identified by a URL, whose cached metadata document declares the redirect URI "https://cimd.example.test/cb" for the next 24 hours
    And my PKCE verifier is "verifier-cimd"
    When I ask to authorize that client
    Then the app answers 400

  @legacy:test_oauth2_cimd::cimd_authorize_refetches_after_cache_expiry
  Scenario: An expired metadata document is fetched again, and its failure is not hidden by the stale copy
    Given an OAuth2 client identified by a URL, whose cached metadata document declares the redirect URI "http://localhost:12345/callback" and expired an hour ago
    And my PKCE verifier is "verifier-cimd"
    When I ask to authorize that client
    Then the app answers 400
    And the response header "location" does not contain "consent"

  @legacy:test_oauth2_cimd::well_known_advertises_cimd
  Scenario: The authorization server metadata advertises the support of metadata documents
    When I request "/.well-known/oauth-authorization-server"
    Then the app answers 200
    And the response field "client_id_metadata_document_supported" is:
      """
      true
      """
