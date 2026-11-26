Feature: Fetching the metadata document of an OAuth2 client identified by a URL
  When the app has no fresh metadata document in its cache for a client identified by a URL,
  it fetches the document: once, as JSON, within a time and a size limit, and without
  following a redirect. A fetched document is cached by its URL, for every user. A fetch
  that fails refuses the authorization and caches nothing. Here the app fetches from a
  local server, in place of the host of the URL.

  Background:
    Given the app fetches the metadata documents of OAuth2 clients from a local server, giving up after 1 second
    And I am signed in
    And I have an API token
    And my PKCE verifier is "verifier-cimd"
    And an OAuth2 client identified by the URL of that metadata document

  Rule: A served document is accepted and cached by its URL

    Scenario: A document that is served is fetched once, accepted and cached for the time it asks
      Given the local server serves once the metadata document, to cache for 7200 seconds
      And the local server is not asked anything else
      When I ask to authorize that client
      Then the app answers 302
      And the app redirects to the consent screen of the front end, for a new request and without a code
      And the metadata document is cached as it was served, for 7200 seconds
      And the local server was asked exactly what it expected

    Scenario: The document that I made the app fetch is not fetched again for another user, who gets a request of their own
      Given the local server serves once the metadata document, to cache for 7200 seconds
      And the local server is not asked anything else
      And another user is signed in
      And the other user has an API token of their own
      When I ask to authorize that client
      And that other user asks to authorize that client
      Then that other user is sent to the consent screen, for a request that is not mine
      And the consent screen of that other user's request shows that client to them, and is refused to me
      And the local server was asked exactly what it expected

    Scenario: A cached document that expired is fetched again and replaced
      Given an OAuth2 client identified by a URL, whose cached metadata document declares the redirect URI "https://cimd.example.test/cb" and expired an hour ago
      And the local server serves once the metadata document, to cache for 7200 seconds
      And the local server is not asked anything else
      When I ask to authorize that client
      Then the app answers 302
      And the app redirects to the consent screen of the front end, for a new request and without a code
      And the metadata document is cached as it was served, for 7200 seconds
      And the local server was asked exactly what it expected

  Rule: A fetch that does not give the document refuses the authorization

    Scenario: A redirect is not followed
      Given the local server answers once to the fetch of the metadata document with a redirect to "/moved.json", where it would serve the document but is never asked
      And the local server is not asked anything else
      When I ask to authorize that client
      Then the app answers 400
      And the response has no "location" header
      And the error message is "Invalid input data: CIMD fetch of https://cimd.example.test/client.json returned HTTP 302 Found, expected 200"
      And no metadata document is cached for that client
      And the local server was asked exactly what it expected

    Scenario: A document that is not found is refused
      Given the local server answers once 404 to the fetch of the metadata document
      And the local server is not asked anything else
      When I ask to authorize that client
      Then the app answers 400
      And the error message is "Invalid input data: CIMD fetch of https://cimd.example.test/client.json returned HTTP 404 Not Found, expected 200"
      And no metadata document is cached for that client
      And the local server was asked exactly what it expected

    Scenario: A document that takes longer than the time limit is refused
      Given the local server serves once the metadata document after 3 seconds
      And the local server is not asked anything else
      When I ask to authorize that client
      Then the app answers 400
      And the response has no "location" header
      And the error message mentions "Failed to fetch CIMD document at https://cimd.example.test/client.json: "
      And no metadata document is cached for that client
      And the local server was asked exactly what it expected

  Rule: Only a JSON document within the size limit is accepted

    Scenario: A document of 5120 bytes, the size limit, is accepted
      Given the local server serves once a metadata document of 5120 bytes
      And the local server is not asked anything else
      When I ask to authorize that client
      Then the app answers 302
      And the app redirects to the consent screen of the front end, for a new request and without a code
      And the local server was asked exactly what it expected

    Scenario: A document of 5121 bytes, one over the size limit, is refused
      Given the local server serves once a metadata document of 5121 bytes
      And the local server is not asked anything else
      When I ask to authorize that client
      Then the app answers 400
      And the error message is "Invalid input data: CIMD document exceeds 5120-byte size cap"
      And no metadata document is cached for that client
      And the local server was asked exactly what it expected

    Scenario: A document served as JSON with a charset is accepted
      Given the local server serves once the metadata document with the content type "application/json; charset=utf-8"
      And the local server is not asked anything else
      When I ask to authorize that client
      Then the app answers 302
      And the app redirects to the consent screen of the front end, for a new request and without a code
      And the local server was asked exactly what it expected

    Scenario: A document that is not served as JSON is refused
      Given the local server serves once the metadata document with the content type "text/html"
      And the local server is not asked anything else
      When I ask to authorize that client
      Then the app answers 400
      And the error message is "Invalid input data: CIMD document at https://cimd.example.test/client.json has non-JSON content-type 'text/html'"
      And no metadata document is cached for that client
      And the local server was asked exactly what it expected
