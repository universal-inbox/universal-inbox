Feature: Syncing Google Mail invitations
  A calendar invitation found in a Google Mail thread becomes a Google Calendar
  notification, found through the iCalUID of the invitation in the user's
  calendar.

  Background:
    Given I am signed in
    And I am connected to Google Calendar
    And I am connected to Google Mail

  Scenario: A new invitation becomes a Google Calendar notification
    Given Google Mail reports a thread with an invitation
    And Google Mail lists its threads over two pages
    And Google Calendar knows the event of that invitation
    When I sync my Google Mail notifications
    Then I get one Google Calendar notification, unread and never read
    And it is the event "eventid1", "Weekly meeting"
    And it repeats every week
    And it comes from the Google Mail thread Google Mail reported

  Scenario: An invitation to a known event updates its notification
    Given Google Mail reports a thread with an invitation
    And Google Mail lists its threads over two pages
    And Google Calendar knows the event of that invitation
    And I already have the notification "mine" of that event, from an earlier Google Mail thread
    When I sync my Google Mail notifications
    Then I get one Google Calendar notification, unread
    And it is the event "eventid1", "Weekly meeting"
    And it repeats every week
    And it is the notification "mine" of the same Google Calendar event, now fed by the new Google Mail thread

  Scenario: A reply to an invitation becomes a Google Calendar notification
    Given Google Mail reports a thread with a reply to an invitation
    And Google Mail lists its threads over two pages
    And Google Calendar knows the event of that reply
    When I sync my Google Mail notifications
    Then I get one Google Calendar notification, unread and never read
    And it is the event of the reply as Google Calendar reports it, with the method reply
    And it comes from the Google Mail thread Google Mail reported

  Scenario: The event of an invitation is found on a later page of Google Calendar
    Given Google Mail reports a thread with an invitation
    And Google Mail lists its threads over two pages
    And Google Calendar answers with an empty page and a next page token, and knows the event on the second page
    When I sync my Google Mail notifications
    Then I get one Google Calendar notification of the event "eventid1", unread

  Scenario: A moved occurrence of a recurring event gets its own notification
    Given Google Mail reports a thread with an invitation to a moved occurrence of a recurring event
    And Google Mail lists its threads over two pages
    And Google Calendar knows the recurring event and its moved occurrence
    When I sync my Google Mail notifications
    Then I get one Google Calendar notification of the event "eventid1_20240510T131500Z", unread
    And it is the event "eventid1_20240510T131500Z", "Weekly meeting (moved)"
    And it does not repeat

  Rule: A cancellation deletes the notification of an invitation I have not answered

    Scenario Outline: A cancellation of an invitation <answer>
      Given Google Mail reports a thread with the cancellation of an invitation
      And Google Mail lists its threads over two pages
      And I already have the notification "mine" of that invitation, <answer>
      And Google Calendar reports that event as cancelled
      When I sync my Google Mail notifications
      Then I get one Google Calendar notification, <status>
      And it is "mine", which I already had
      And it is the cancelled event "eventid1"

      Examples:
        | answer                       | status  |
        | which I have not answered    | deleted |

      Examples:
        | answer             | status |
        | which I accepted   | unread |

    Scenario: A cancellation of an invitation I never received
      Given Google Mail reports a thread with the cancellation of an invitation
      And Google Mail lists its threads over two pages
      And I never received that invitation
      And Google Calendar reports that event as cancelled
      When I sync my Google Mail notifications
      Then I get one Google Calendar notification, unread
      And it is the cancelled event "eventid1"
