Feature: Syncing Google Mail invitations
  A calendar invitation found in a Google Mail thread becomes a Google Calendar
  notification, found through the iCalUID of the invitation in the user's
  calendar.

  Background:
    Given I am signed in
    And I am connected to Google Calendar
    And I am connected to Google Mail

  @legacy:test_sync_google_mail_threads::test_sync_notifications_should_create_a_new_google_calendar_notification_from_a_google_mail_invitation
  Scenario: A new invitation becomes a Google Calendar notification
    Given Google Mail reports a thread with an invitation
    And Google Mail lists its threads over two pages
    And Google Calendar knows the event of that invitation
    When I sync my Google Mail notifications
    Then I get one Google Calendar notification, unread and never read
    And it is the event "eventid1", "Weekly meeting"
    And it repeats every week
    And it comes from the Google Mail thread Google Mail reported

  @legacy:test_sync_google_mail_threads::test_sync_notifications_should_update_a_google_calendar_notification_from_a_google_mail_invitation_update
  Scenario: An invitation to a known event updates its notification
    Given Google Mail reports a thread with an invitation
    And Google Mail lists its threads over two pages
    And Google Calendar knows the event of that invitation
    And I already have the notification "mine" of that event, from an earlier Google Mail thread
    When I sync my Google Mail notifications
    Then I get one Google Calendar notification, unread
    And it is the notification "mine" of the same Google Calendar event, now fed by the new Google Mail thread

  @legacy:test_sync_google_mail_threads::test_sync_notifications_should_create_a_new_google_calendar_notification_from_a_google_mail_invitation_reply
  Scenario: A reply to an invitation becomes a Google Calendar notification
    Given Google Mail reports a thread with a reply to an invitation
    And Google Mail lists its threads over two pages
    And Google Calendar knows the event of that reply
    When I sync my Google Mail notifications
    Then I get one Google Calendar notification, unread and never read
    And it is the event of the reply as Google Calendar reports it, with the method reply
    And it comes from the Google Mail thread Google Mail reported

  @legacy:test_sync_google_mail_threads::test_sync_notifications_should_find_the_google_calendar_event_of_an_invitation_on_a_later_page
  Scenario: The event of an invitation is found on a later page of Google Calendar
    Given Google Mail reports a thread with an invitation
    And Google Mail lists its threads over two pages
    And Google Calendar answers with an empty page and a next page token, and knows the event on the second page
    When I sync my Google Mail notifications
    Then I get one Google Calendar notification of the event "eventid1", unread

  @legacy:test_sync_google_mail_threads::test_sync_notifications_should_create_a_google_calendar_notification_for_the_updated_occurrence_of_a_recurring_event
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

      @legacy:test_sync_google_mail_threads::test_sync_notifications_should_handle_a_cancelled_google_calendar_event_from_a_google_mail_invitation::case_1_cancel_a_pending_invitation
      Examples:
        | answer                       | status  |
        | which I have not answered    | deleted |

      @legacy:test_sync_google_mail_threads::test_sync_notifications_should_handle_a_cancelled_google_calendar_event_from_a_google_mail_invitation::case_2_cancel_an_accepted_invitation
      Examples:
        | answer             | status |
        | which I accepted   | unread |

    @legacy:test_sync_google_mail_threads::test_sync_notifications_should_handle_a_cancelled_google_calendar_event_from_a_google_mail_invitation::case_3_cancel_an_unknown_invitation
    Scenario: A cancellation of an invitation I never received
      Given Google Mail reports a thread with the cancellation of an invitation
      And Google Mail lists its threads over two pages
      And I never received that invitation
      And Google Calendar reports that event as cancelled
      When I sync my Google Mail notifications
      Then I get one Google Calendar notification, unread
      And it is the cancelled event "eventid1"
