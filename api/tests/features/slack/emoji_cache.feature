Feature: Caching the Slack emojis
  The emojis of a Slack workspace are cached per credential: two tokens never
  share a cache entry, even without a team id, and one token asks Slack once.

  Scenario: Two credentials without a team id do not share their emojis
    Given workspace "a" has a Slack token without a team id, and Slack knows the emoji "secret_a" and is asked for it once
    And workspace "b" has a Slack token without a team id, and Slack knows the emoji "secret_b" and is asked for it once
    When I list the emojis of workspace "a"
    And I list the emojis of workspace "b"
    Then the emojis listed for workspace "a" are only "secret_a"
    And the emojis listed for workspace "b" are only "secret_b"

  Scenario: Listing the emojis twice with one credential asks Slack once
    Given workspace "a" has a Slack token without a team id, and Slack knows the emoji "party" and is asked for it once
    When I list the emojis of workspace "a"
    And I list the emojis of workspace "a"
    Then the emojis listed for workspace "a" are only "party"
    And both lists are the same
