Feature: Slack revoking the access of the app
  Slack tells when the access of a user is revoked, or when the whole app is uninstalled
  from a workspace: the connections concerned fail, with their credential removed, and the
  others are left alone. The events can come twice or in any order, a late one does not
  fail a connection that was made after it, and a paused connection stays paused.

  Background:
    Given I am signed in

  Rule: Which connections a revocation reaches

    @legacy:test_slack_webhook_revocation::test_tokens_revoked_fails_only_the_revoked_user_connection
    Scenario: The access of one Slack user is revoked, and only their connection fails
      Given I have the Slack connection "mine" of the Slack user "U05XXX" in the team "T05XXX"
      And another user has the Slack connection "jane" of the Slack user "U06XXX" in the team "T05XXX"
      When Slack sends the tokens_revoked event of the team "T05XXX" for the Slack users "U05XXX"
      Then Slack is told that the event was received
      And the Slack connection "mine" failed because its access was revoked, and its credential is gone
      And the Slack connection "jane" is still validated, without failure, with its credential

    @legacy:test_slack_webhook_revocation::test_app_uninstalled_fails_every_connection_of_the_workspace
    Scenario: Uninstalling the app fails every connection of the workspace and no other
      Given I have the Slack connection "mine" of the Slack user "U05XXX" in the team "T05XXX"
      And another user has the Slack connection "jane" of the Slack user "U06XXX" in the team "T05XXX"
      And another user has the Slack connection "jim" of the Slack user "U07XXX" in the team "T06XXX"
      When Slack sends the app_uninstalled event of the team "T05XXX"
      Then Slack is told that the event was received
      And the Slack connection "mine" failed because its access was revoked, and its credential is gone
      And the Slack connection "jane" failed because its access was revoked, and its credential is gone
      And the Slack connection "jim" is still validated, without failure, with its credential

    @legacy:test_slack_webhook_revocation::test_tokens_revoked_for_unknown_user_changes_nothing
    Scenario: Revocations that are not about my connection change nothing
      Given I have the Slack connection "mine" of the Slack user "U05XXX" in the team "T05XXX"
      When Slack sends the tokens_revoked event of the team "T05XXX" for the Slack users "U99XXX"
      And Slack sends the tokens_revoked event of the team "T06XXX" for the Slack users "U05XXX"
      And Slack sends the tokens_revoked event of the team "T05XXX" for the Slack users ""
      And Slack sends the app_uninstalled event of the team "T06XXX"
      Then Slack is told that the event was received
      And the Slack connection "mine" is still validated, without failure, with its credential

  Rule: Events that come twice, late or at the wrong time

    @legacy:test_slack_webhook_revocation::test_replayed_and_interleaved_revocation_events_are_idempotent
    Scenario: Replayed and interleaved revocation events leave the connection revoked
      Given I have the Slack connection "mine" of the Slack user "U05XXX" in the team "T05XXX"
      When Slack sends the app_uninstalled event of the team "T05XXX"
      Then the Slack connection "mine" failed because its access was revoked, and its credential is gone
      When Slack sends the tokens_revoked event of the team "T05XXX" for the Slack users "U05XXX"
      Then the Slack connection "mine" failed because its access was revoked, and its credential is gone
      When Slack sends the tokens_revoked event of the team "T05XXX" for the Slack users "U05XXX"
      Then the Slack connection "mine" failed because its access was revoked, and its credential is gone
      When Slack sends the app_uninstalled event of the team "T05XXX"
      Then Slack is told that the event was received
      And the Slack connection "mine" failed because its access was revoked, and its credential is gone

    @legacy:test_slack_webhook_revocation::test_late_revocation_event_spares_a_reconnected_connection
    Scenario: Revocation events that come five minutes late spare a connection made after them
      Given I have the Slack connection "mine" of the Slack user "U05XXX" in the team "T05XXX"
      When Slack sends, 5 minutes late, the tokens_revoked event of the team "T05XXX" for the Slack users "U05XXX"
      And Slack sends, 5 minutes late, the app_uninstalled event of the team "T05XXX"
      Then Slack is told that the event was received
      And the Slack connection "mine" is still validated, without failure, with its credential

    @legacy:test_slack_webhook_revocation::test_revocation_event_leaves_a_paused_connection_paused
    Scenario: Revocation events leave a paused connection paused
      Given I have the Slack connection "mine" of the Slack user "U05XXX" in the team "T05XXX"
      And my Slack connection "mine" is paused for inactivity, and its credential is gone
      When Slack sends the tokens_revoked event of the team "T05XXX" for the Slack users "U05XXX"
      And Slack sends the app_uninstalled event of the team "T05XXX"
      Then Slack is told that the event was received
      And the Slack connection "mine" is still paused, without failure, and without credential

  Rule: Only Slack can revoke a connection

    Scenario Outline: A <kind> revocation without a valid signature is refused and changes nothing
      Given I have the Slack connection "mine" of the Slack user "U05XXX" in the team "T05XXX"
      When <unsigned>
      And <wrongly signed>
      Then the Slack connection "mine" is still validated, without failure, with its credential
      When <signed>
      Then the Slack connection "mine" failed because its access was revoked, and its credential is gone

      Examples:
        | kind            | unsigned                                                                                                          | wrongly signed                                                                                                           | signed                                                                                      |
        | tokens_revoked  | an attacker sends the tokens_revoked event of the team "T05XXX" for the Slack users "U05XXX", without a signature | an attacker sends the tokens_revoked event of the team "T05XXX" for the Slack users "U05XXX", signed with a wrong secret | Slack sends the tokens_revoked event of the team "T05XXX" for the Slack users "U05XXX"      |
        | app_uninstalled | an attacker sends the app_uninstalled event of the team "T05XXX", without a signature                              | an attacker sends the app_uninstalled event of the team "T05XXX", signed with a wrong secret                              | Slack sends the app_uninstalled event of the team "T05XXX"                                   |
