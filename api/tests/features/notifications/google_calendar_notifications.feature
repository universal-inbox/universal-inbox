Feature: Google Calendar notifications
  The notification of a Google Calendar invitation is mirrored by the Google Mail thread
  that carried it. Deleting it archives the thread and leaves the event alone,
  unsubscribing from it also deletes the event, and an answer to the invitation is sent
  to Google Calendar. The reply to an invitation cannot be answered.

  Background:
    Given I am signed in
    And I am connected to Google Mail
    And I am connected to Google Calendar

  Rule: A status change of the notification of an invitation

    Scenario: Deleting the notification archives the Google Mail thread and leaves the event alone
      Given I already have the notification "invitation" of that invitation, which I have not answered
      And Google Mail archives, once, the thread of "invitation"
      And Google Calendar is not asked anything
      When I set the status of "invitation" to deleted
      Then the patched notification is deleted, was read just now, and is otherwise as "invitation" was

    Scenario: Unsubscribing from the notification archives the Google Mail thread and deletes the event
      Given I already have the notification "invitation" of that invitation, which I have not answered
      And Google Mail archives, once, the thread of "invitation"
      And Google Calendar deletes, once, the event of "invitation"
      When I set the status of "invitation" to unsubscribed
      Then the patched notification is unsubscribed, was read just now, and is otherwise as "invitation" was

    Scenario: Snoozing the notification reaches neither Google Mail nor Google Calendar
      Given I already have the notification "invitation" of that invitation, which I have not answered
      And Google Mail is not asked anything
      And Google Calendar is not asked anything
      When I snooze "invitation" until "2022-01-01T01:02:03Z"
      Then the patched notification is "invitation" snoozed until "2022-01-01T01:02:03Z"

  Rule: Answering the invitation

    Scenario Outline: The invitation is <answer>
      Given I already have the notification "invitation" of that invitation, which I have not answered
      And Google Mail archives, once, the thread of "invitation"
      And Google Calendar records, once, my answer <answer> to the event of "invitation"
      When I answer the invitation of "invitation" with <answer>
      Then the answered notification is deleted, was read just now, and its event carries my answer <answer>
      And "invitation", read again, has the event with my answer <answer>

      Examples:
        | answer   |
        | accepted |

      Examples:
        | answer   |
        | declined |

      Examples:
        | answer    |
        | tentative |

    Scenario: The reply to an invitation cannot be answered
      Given I already have the notification "reply" of a reply to an invitation
      And Google Mail is not asked anything
      And Google Calendar is not asked anything
      When I answer the invitation of "reply" with accepted
      Then the app answers 400
      And "reply", read again, keeps its status

  Rule: Another user cannot act on my notification of an invitation

    Background:
      Given I already have the notification "invitation" of that invitation, which I have not answered
      And another user is signed in
      And that other user has their own Google Mail and Google Calendar connections, with access tokens of their own, and the notification "theirs" of the same invitation
      And Google Mail is not asked anything
      And Google Calendar is not asked anything

    Scenario: Another user cannot answer my invitation, and their own notification of it is left alone
      When that other user answers the invitation of "invitation" with accepted
      Then the app answers 403
      And "invitation", read again, is exactly as it was
      And the notification "theirs" of that other user, read by that other user, is exactly as it was

    Scenario: Another user cannot unsubscribe me from my invitation, which would delete the event
      When that other user tries to set the status of "invitation" to unsubscribed
      Then the app answers 403
      And "invitation", read again, is exactly as it was
      And the notification "theirs" of that other user, read by that other user, is exactly as it was
