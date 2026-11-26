Feature: Syncing Google Drive comments
  A sync lists the files modified since the connection was created, reads their
  comments and brings in the new ones. A comment Jane Doe already answered is
  deleted, new replies of an unsubscribed comment only reopen it when they
  mention the user, and a file without comments resource does not abort the sync.

  Background:
    Given I am signed in
    And I am connected to Google Drive
    And Google Drive knows me as "Jane Doe", with the address "jane.doe@example.com"

  Scenario: A sync adds the new comments and updates the known one without losing my triage
    Given I am connected to Todoist
    And Todoist can list my projects
    And I already have the Google Drive notification "existing" of the comment 456
    And I have a Todoist task in the project "Project2", linked to "existing"
    And I snooze "existing" until "2064-01-01T00:00:00Z"
    And Google Drive lists two files, over two pages
    And Google Drive reports the fixture comments on the first file, both modified in five minutes
    And Google Drive reports no comment on the second file
    When I sync my Google Drive notifications
    Then my notifications are the comments Google Drive reported, for the user Jane Doe
    And "existing" is deleted, as Jane Doe already answered, and now holds the comment with her name
    And "existing" is still snoozed until "2064-01-01T00:00:00Z"
    And "existing" is still linked to my Todoist task
    And my Google Drive connection knows me as "Jane Doe", with the address "jane.doe@example.com"

  Scenario Outline: A new reply in a comment I unsubscribed from, <mention> and <newness> than my notification
    Given I already have the Google Drive notification "mine" of the comment 123
    And I unsubscribe from "mine"
    And Google Drive lists a single file
    And Google Drive reports a single comment on the first file, whose only reply is <mention> and <newness> than my notification
    When I sync my Google Drive notifications
    Then the sync returned <returned> notification
    And my Google Drive notification "mine" is <status>

    Examples:
      | mention           | newness | returned | status       |
      | not mentioning me | newer   | 0        | unsubscribed |

    Examples:
      | mention           | newness | returned | status       |
      | not mentioning me | older   | 0        | unsubscribed |

    Examples:
      | mention       | newness | returned | status |
      | mentioning me | newer   | 1        | unread |


    Examples:
      | mention       | newness | returned | status       |
      | mentioning me | older   | 0        | unsubscribed |

  Scenario: A file without comments resource does not abort the sync
    Given Google Drive lists two files, on a single page
    And Google Drive reports the fixture comments on the first file
    And Google Drive answers that the second file has no comments resource
    When I sync my Google Drive notifications
    Then the sync returned one unread Google Drive notification

  Scenario: A comment I already know, unchanged, is not synced again
    Given I already have the Google Drive notification "mine" of the comment 123
    And Google Drive lists a single file
    And Google Drive reports the first fixture comment alone, unchanged
    When I sync my Google Drive notifications
    Then the sync returned 0 notification
    And my Google Drive notification "mine" is unread
    And "mine" was not rewritten by the sync
