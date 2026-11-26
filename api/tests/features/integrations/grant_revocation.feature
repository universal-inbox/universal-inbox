Feature: Revoking provider grants
  Disconnecting a provider, or deleting the account, revokes the grant the app
  holds at the provider. A revocation the provider refuses is kept and retried
  with a growing delay, instead of leaving a live token behind.

  Background:
    Given I am signed in

  Rule: Disconnecting a provider revokes the grant

    Scenario Outline: Disconnecting <provider> revokes the grant
      Given I have a validated <provider> connection with a stored credential
      And <provider> revokes my grant when asked
      When I disconnect it
      Then <provider> was asked once to revoke my grant

      Examples:
        | provider    |
        | GitHub      |

      Examples:
        | provider    |
        | Google Mail |

      Examples:
        | provider    |
        | Slack       |

      Examples:
        | provider    |
        | Linear      |

      Examples:
        | provider    |
        | TickTick    |

      Examples:
        | provider    |
        | Notion      |

  Rule: A revocation the provider refuses is retried

    Scenario: A refused revocation is queued, then revoked at the next retry
      Given I have a validated GitHub connection with a stored credential
      And GitHub fails to revoke my grant
      When I disconnect it
      Then GitHub was asked once to revoke my grant, and failed
      And the stored revocations are
        | status  | attempts | tokens | connection |
        | Pending | 1        | kept   | mine       |
      When the provider is back
      And GitHub revokes my grant when asked
      And the waiting revocations come due
      When the revocation retry job runs, allowing 20 attempts
      Then the revocation retry job completed 1 and failed 0
      And GitHub was asked once to revoke my grant
      And the stored revocations are
        | status  | attempts | tokens | connection |
        | Revoked | 1        | erased | mine       |

    Scenario: A revocation that keeps failing backs off, then is abandoned with its tokens kept
      Given I have a validated GitHub connection with a stored credential
      And GitHub fails to revoke my grant
      When I disconnect it
      And the waiting revocations come due
      And the revocation retry job runs, allowing 3 attempts
      Then the revocation retry job completed 0 and failed 1
      And the stored revocations are
        | status  | attempts | tokens | connection |
        | Pending | 2        | kept   | mine       |
      When the revocation retry job runs, allowing 3 attempts
      Then the revocation retry job completed 0 and failed 0
      When the waiting revocations come due
      And the revocation retry job runs, allowing 3 attempts
      Then the revocation retry job completed 0 and failed 1
      And the stored revocations are
        | status    | attempts | tokens | connection |
        | Abandoned | 3        | kept   | mine       |

    Scenario: Connecting again cancels the queued revocation, which would kill the new token
      Given I have a validated GitHub connection with a stored credential
      And GitHub fails to revoke my grant
      When I disconnect it
      And the provider is back
      And I connect it again
      And GitHub revokes my grant when asked
      And the waiting revocations come due
      And the revocation retry job runs, allowing 20 attempts
      Then the revocation retry job completed 1 and failed 0
      And GitHub was not asked to revoke my grant
      And the stored revocations are
        | status    | attempts | tokens | connection |
        | Cancelled | 1        | erased | mine       |

  Rule: Slack tokens that rotate

    Scenario: An expired Slack token is renewed, then revoked
      Given I have a validated Slack connection whose credential rotates
      And its access token expired an hour ago
      And Slack renews my token when asked
      And Slack accepts to revoke my renewed token
      When I disconnect it
      Then Slack was asked once to renew my token
      And Slack was asked once to revoke my grant
      And no revocation is stored

    Scenario: A queued Slack token that expired meanwhile is renewed at the retry, then revoked
      Given I have a validated Slack connection whose credential rotates
      And Slack is unavailable when asked to revoke a token
      When I disconnect it
      Then the stored revocations are
        | status  | attempts | tokens | connection |
        | Pending | 1        | kept   | mine       |
      When the provider is back
      And Slack answers that my token expired
      And Slack renews my token when asked
      And Slack accepts to revoke my renewed token
      And the waiting revocations come due
      When the revocation retry job runs, allowing 20 attempts
      Then the revocation retry job completed 1 and failed 0
      And Slack was asked once about my expired token
      And Slack was asked once to renew my token
      And Slack was asked once to revoke my grant
      And the stored revocations are
        | status  | attempts | tokens | connection |
        | Revoked | 1        | erased | mine       |

    Scenario: A Slack token that is already dead counts as revoked
      Given I have a validated Slack connection with a stored credential
      And Slack answers that my token was already revoked
      When I disconnect it
      Then Slack was asked once to revoke my grant
      And no revocation is stored

  Rule: Deleting the account revokes every grant

    Scenario: Deleting the account revokes the grants of every connection
      Given I have a validated GitHub connection with a stored credential
      And I have a validated Google Mail connection with a stored credential
      And GitHub revokes my grant when asked
      And Google Mail revokes my grant when asked
      When I delete my account, confirming with my email address
      Then the app answers 200
      And GitHub was asked once to revoke my grant
      And Google Mail was asked once to revoke my grant

    Scenario: Deleting the account keeps a refused revocation, though its connection is gone
      Given I have a validated GitHub connection with a stored credential
      And GitHub fails to revoke my grant
      When I delete my account, confirming with my email address
      Then the app answers 200
      And the stored revocations are
        | status  | attempts | tokens | connection |
        | Pending | 1        | kept   | none       |
      When the provider is back
      And GitHub revokes my grant when asked
      And the waiting revocations come due
      When the revocation retry job runs, allowing 20 attempts
      Then the revocation retry job completed 1 and failed 0
      And GitHub was asked once to revoke my grant
      And the stored revocations are
        | status  | attempts | tokens | connection |
        | Revoked | 1        | erased | none       |
