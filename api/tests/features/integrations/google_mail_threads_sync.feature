Feature: Syncing Google Mail threads
  A sync brings in the threads that carry the label to synchronise, adds the new
  ones and updates the known ones without undoing what the user did in Universal
  Inbox. A thread the user answered is deleted, and new messages of an
  unsubscribed thread are archived unless one is addressed to the user.

  Background:
    Given I am signed in
    And I am connected to Google Mail

  @legacy:test_sync_google_mail_threads::test_sync_notifications_should_add_new_notification_and_update_existing_one
  Scenario: A sync adds the new threads and updates the known one without losing my triage
    Given I am connected to Todoist
    And Todoist can list my projects
    And Google Mail reports the threads 123 and 456
    And Google Mail lists its threads over two pages
    And I already have the notification "existing" of the Google Mail thread 456
    And I have a Todoist task in the project "Project2", linked to "existing"
    And I snooze "existing" until "2064-01-01T00:00:00Z"
    When I sync my Google Mail notifications
    Then my notifications are the threads Google Mail reported
    And "existing" is read since "2023-09-13T20:27:16Z"
    And "existing" is still snoozed until "2064-01-01T00:00:00Z"
    And "existing" is still linked to my Todoist task
    And my Google Mail connection learned my address and the labels Google Mail reported

  Rule: New messages of a thread I unsubscribed from are archived, unless one is addressed to me

    Scenario Outline: A new message addressed to <addressee>, <reading>
      Given Google Mail reports the thread 456
      And I unsubscribed from the Google Mail thread 456, "mine", before its second message arrived
      And that second message is addressed to <addressee> and is <reading>
      When I sync my Google Mail notifications
      Then "mine" ends up <status>
      And the stored thread of "mine" has two messages, the first one without any label
      And the second message of "mine" keeps the labels "<labels>"
      And the number of archive requests Google Mail received is <archive requests>

      @legacy:test_sync_google_mail_threads::test_sync_notifications_of_unsubscribed_notification_with_new_messages::case_1
      Examples:
        | addressee     | reading | status       | labels             | archive requests |
        | somebody else | unread  | unsubscribed | TEST_LABEL, UNREAD | 1                |

      @legacy:test_sync_google_mail_threads::test_sync_notifications_of_unsubscribed_notification_with_new_messages::case_2
      Examples:
        | addressee     | reading | status       | labels     | archive requests |
        | somebody else | read    | unsubscribed | TEST_LABEL | 1                |

      @legacy:test_sync_google_mail_threads::test_sync_notifications_of_unsubscribed_notification_with_new_messages::case_3
      Examples:
        | addressee | reading | status | labels                                      | archive requests |
        | me        | unread  | unread | TEST_LABEL, INBOX, the synced label, UNREAD | 0                |

      @legacy:test_sync_google_mail_threads::test_sync_notifications_of_unsubscribed_notification_with_new_messages::case_4
      Examples:
        | addressee | reading | status       | labels     | archive requests |
        | me        | read    | unsubscribed | TEST_LABEL | 1                |

  Rule: A thread whose last message I sent is deleted

    Scenario Outline: A thread whose last message was sent by <sender>
      Given Google Mail reports the thread 456
      And the last message of that thread is unread and was sent by <sender>
      When I sync my Google Mail notifications
      Then I get one notification of the Google Mail thread 456, that ends up <status>

      @legacy:test_sync_google_mail_threads::test_sync_notifications_should_mark_notification_as_deleted_when_user_replied::case_1_sent_label
      Examples:
        | sender | status  |
        | me     | deleted |

      @legacy:test_sync_google_mail_threads::test_sync_notifications_should_mark_notification_as_deleted_when_user_replied::case_2_no_sent_label
      Examples:
        | sender     | status |
        | a stranger | unread |

      @legacy:test_sync_google_mail_threads::test_sync_notifications_should_mark_notification_as_deleted_when_user_replied::case_3_spoofed_display_name_without_sent_label
      Examples:
        | sender                                            | status |
        | a stranger who put my address in the display name | unread |
