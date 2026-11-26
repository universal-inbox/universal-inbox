Feature: Acting on a GitHub notification
  Dismissing a GitHub notification is done at GitHub too: its thread is marked
  as read and deleted, and unsubscribing also drops the subscription. The user's
  last read marker moves forward, GitHub's own being left behind, and a failure
  at GitHub leaves the notification as it was.

  Background:
    Given I am signed in
    And I am connected to GitHub

  Rule: Deleting a notification

    Scenario Outline: A deleted notification is deleted at GitHub, whatever it answers with <status>
      Given I am connected to Todoist
      And Todoist can list my projects
      And I have a GitHub notification called "mine"
      And I have a Todoist task in the project "Project2", linked to "mine"
      And GitHub answers <status> when asked to mark the thread as read and to delete it
      And GitHub would accept to unsubscribe from the thread
      When I mark "mine" as deleted
      Then the patched notification is "mine", deleted, with its read marker moved forward
      And GitHub was asked once to mark the thread as read, and once to delete it
      And GitHub was not asked to unsubscribe from the thread
      And the Todoist task keeps its status

      Examples:
        | status |
        | 205    |

      Examples:
        | status |
        | 304    |

      Examples:
        | status |
        | 404    |

    Scenario: A notification GitHub refuses to delete stays unread
      Given I have a GitHub notification called "mine"
      And GitHub refuses to delete the thread
      When I mark "mine" as deleted
      Then the app answers 500
      And the app says an internal error occurred, and asks to retry later or contact support
      And "mine" is still unread

  Rule: Unsubscribing from a notification

    Scenario: An unsubscribed notification is marked as read, deleted and unsubscribed at GitHub
      Given I have a GitHub notification called "mine"
      And GitHub accepts to mark the thread as read and to delete it
      And GitHub would accept to unsubscribe from the thread
      When I mark "mine" as unsubscribed
      Then the patched notification is "mine", unsubscribed, with its read marker moved forward
      And GitHub was asked once to mark the thread as read, and once to delete it
      And GitHub was asked once to unsubscribe from the thread

  Rule: Changing a GitHub notification without acting on GitHub

    Scenario: A user snoozes a GitHub notification
      Given I have a GitHub notification called "mine"
      When I snooze "mine" until "2022-01-01T01:02:03Z"
      Then the patched notification is "mine" snoozed until "2022-01-01T01:02:03Z"

    Scenario: A change that changes nothing on a GitHub notification is not modified
      Given I have a GitHub notification called "mine"
      When I snooze "mine" until "2022-01-01T01:02:03Z"
      And I send "mine" the status it already has and the snooze time "2022-01-01T01:02:03Z"
      Then the app answers 304

    Scenario: A change without any value is refused
      Given I have a GitHub notification called "mine"
      When I send "mine" a patch with nothing to change
      Then the app answers 400

    Scenario: A notification that does not exist cannot be changed
      When I mark a notification that does not exist as deleted
      Then the app answers 404
      And the error message is "Cannot update unknown notification <id>", with <id> being the id of that unknown notification
