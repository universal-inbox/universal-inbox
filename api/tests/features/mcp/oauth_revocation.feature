Feature: Revoking the authorization of an OAuth2 client
  Revoking a client kills its outstanding authorization codes and its consent, and never
  leaves a live refresh token behind, even when the revocation races a code exchange. The
  other clients of the same user are not touched.

  Background:
    Given I am signed in

  @legacy:test_oauth2_revocation::exchange_after_revocation_fails_and_issues_nothing
  Scenario: A code cannot be exchanged after its client was revoked
    Given I approved the OAuth2 client "A" and was issued an authorization code for it
    When I revoke the authorization of the client "A"
    And I exchange the authorization code of the client "A"
    Then the exchange was refused with "Invalid or expired authorization code"
    And the client "A" has 0 refresh tokens, 0 of them live

  @legacy:test_oauth2_revocation::exchange_of_non_revoked_grant_still_works
  Scenario: A code of a client that was not revoked is exchanged for a live refresh token
    Given I approved the OAuth2 client "A" and was issued an authorization code for it
    When I exchange the authorization code of the client "A"
    Then the exchange issued tokens
    And the client "A" has 1 refresh token, 1 of them live

  @legacy:test_oauth2_revocation::exchange_fails_when_consent_is_gone_even_if_code_survives
  Scenario: A code whose consent is gone cannot be exchanged, even though the code survives
    Given I approved the OAuth2 client "A" and was issued an authorization code for it
    When my consent for the client "A" is deleted, and its authorization code is kept
    And I exchange the authorization code of the client "A"
    Then the exchange was refused with "Invalid or expired authorization code"
    And the client "A" has 0 refresh tokens, 0 of them live

  @legacy:test_oauth2_revocation::revoking_one_client_leaves_other_client_untouched
  Scenario: Revoking one client leaves the other client of the same user untouched
    Given I approved the OAuth2 client "A" and was issued an authorization code for it
    And I approved the OAuth2 client "B" and was issued an authorization code for it
    When I revoke the authorization of the client "A"
    Then the client "A" has 0 authorization codes and 0 consents of mine
    And the client "B" has 1 authorization code and 1 consent of mine
    When I exchange the authorization code of the client "B"
    Then the exchange issued tokens

  @legacy:test_oauth2_revocation::concurrent_revoke_and_exchange_never_leave_a_live_refresh_token
  Scenario: A revocation racing a code exchange never leaves a live refresh token
    When 20 times in a row, I approve a new client, then revoke its authorization while exchanging its code at the same time, interleaved on the single-threaded scenario runtime
    Then none of those clients has a live refresh token

  @legacy:test_oauth2_revocation::codes_issued_after_revocation_and_reconsent_are_exchangeable
  Scenario: A client approved again after its revocation gets codes that can be exchanged
    Given I approved the OAuth2 client "A" and was issued an authorization code for it
    When I revoke the authorization of the client "A"
    And I approve the client "A" again, and am issued a new authorization code
    And I exchange the authorization code of the client "A"
    Then the exchange issued tokens
    And the client "A" has 1 refresh token, 1 of them live

  Scenario: A refresh token that was already issued is killed by the revocation, and kept as a revoked row
    Given I approved the OAuth2 client "A" and was issued an authorization code for it
    When I exchange the authorization code of the client "A"
    Then the exchange issued tokens
    And the client "A" has 1 refresh token, 1 of them live
    When I revoke the authorization of the client "A"
    Then the client "A" has 1 refresh token, 0 of them live
    And the client "A" has 0 authorization codes and 0 consents of mine
    When I use the refresh token that the exchange of the client "A" gave me
    Then the exchange was refused with "Invalid refresh token"

  Scenario: Revoking a client of mine leaves the tokens of another user of the same client alone
    Given I approved the OAuth2 client "A" and was issued an authorization code for it
    And another user is signed in
    And that other user also approved the client "A" and was issued an authorization code for it
    And that other user exchanged their authorization code of the client "A"
    And that other user was issued another authorization code for the client "A"
    When I revoke the authorization of the client "A"
    Then the client "A" has 0 authorization codes and 0 consents of mine
    And the client "A" has 1 refresh token, 1 of them live, 1 authorization code and 1 consent of that other user
    When that other user exchanges that other authorization code of the client "A"
    Then the exchange issued tokens
