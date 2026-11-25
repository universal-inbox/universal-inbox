Feature: Handling a Notion comment event
  The job that handles a Notion comment decides, for each user connected to the workspace
  who can see the page, whether to create, hide, bring back or delete their notification of the discussion.

  Background:
    Given I am signed in

  Rule: A comment creates a notification for the user it concerns

    @legacy:test_notion_webhook::job::test_mention_creates_an_unread_notification
    Scenario: A comment that mentions me creates an unread notification
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text                  |
        | comment-1 | someone | 5      | What do you think @me |
      When the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my unread Notion notifications
      Then my only Notion notification is unread
      And that Notion notification is titled "Comment by <someone> on Roadmap"
      And that Notion notification was updated at "2026-10-10T09:05:00+00:00"
      And that Notion notification is the comment of the discussion, with its quoted content

    @legacy:test_notion_webhook::job::test_comment_on_my_page_or_block_creates_a_notification::case_1_page_creator
    Scenario: A comment on a page that I created creates a notification
      Given I am connected to Notion, in a workspace of its own
      And Notion serves a discussion on a page created by me and a block created by someone, with these comments:
        | comment   | author  | minute | text       |
        | comment-1 | someone | 5      | Looks good |
      When the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my unread Notion notifications
      Then my only Notion notification is unread

    @legacy:test_notion_webhook::job::test_comment_on_my_page_or_block_creates_a_notification::case_2_block_creator
    Scenario: A comment on a block that I created creates a notification
      Given I am connected to Notion, in a workspace of its own
      And Notion serves a discussion on a page created by someone and a block created by me, with these comments:
        | comment   | author  | minute | text       |
        | comment-1 | someone | 5      | Looks good |
      When the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my unread Notion notifications
      Then my only Notion notification is unread

    @legacy:test_notion_webhook::job::test_comment_not_involving_me_is_ignored
    Scenario: A comment that does not involve me creates nothing
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text         |
        | comment-1 | someone | 5      | Note to self |
      When the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my Notion notifications, whatever their status
      Then I see no Notion notification

    @legacy:test_notion_webhook::job::test_event_of_another_workspace_creates_nothing
    Scenario: A comment of a workspace that nobody connected creates nothing
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text |
        | comment-1 | someone | 5      | @me  |
      When the Notion job handles the comment "comment-1" of a workspace that nobody connected
      And I look at my Notion notifications, whatever their status
      Then I see no Notion notification

    @legacy:test_notion_webhook::job::test_page_not_shared_with_me_creates_nothing
    Scenario: A comment on a page that Notion does not share with me creates nothing
      Given I am connected to Notion, in a workspace of its own
      And Notion does not share the page with the token "notion-token-me"
      When the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my Notion notifications, whatever their status
      Then I see no Notion notification

  Rule: The comment sync can be switched off

    @legacy:test_notion_webhook::job::test_disabled_comment_sync_creates_nothing
    Scenario: With the comment sync disabled, a comment creates nothing
      Given I am connected to Notion, with its comment sync disabled
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text |
        | comment-1 | someone | 5      | @me  |
      When the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my Notion notifications, whatever their status
      Then I see no Notion notification

    @legacy:test_notion_webhook::job::test_enabled_comment_sync_applies_to_the_next_comment
    Scenario: Enabling the comment sync applies to the next comment
      Given I am connected to Notion, with its comment sync disabled
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text |
        | comment-1 | someone | 5      | @me  |
      When the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my Notion notifications, whatever their status
      Then I see no Notion notification
      When I enable the comment sync of my Notion connection
      And the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is unread

  Rule: The notification follows the discussion

    @legacy:test_notion_webhook::job::test_my_reply_hides_the_notification_until_someone_answers
    Scenario: My reply hides the notification, and an answer of someone brings it back
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text    |
        | comment-1 | someone | 5      | Ping @me |
        | comment-2 | me      | 10     | Done    |
      When the Notion job handles the comment "comment-2" written by me, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is deleted
      When Notion now serves a discussion with these comments:
        | comment   | author  | minute | text     |
        | comment-1 | someone | 5      | Ping @me |
        | comment-2 | me      | 10     | Done     |
        | comment-3 | someone | 15     | Thanks   |
      And the Notion job handles the comment "comment-3" written by someone, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is unread
      And that Notion notification was updated at "2026-10-10T09:15:00+00:00"

    @legacy:test_notion_webhook::job::test_deleted_discussion_deletes_the_notification
    Scenario: A deleted discussion deletes the notification
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text |
        | comment-1 | someone | 5      | @me  |
      When the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my unread Notion notifications
      Then my only Notion notification is unread
      When Notion now serves a discussion with these comments:
        | comment | author | minute | text |
      And the Notion job handles the deletion of the comment "comment-1"
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is deleted

    @legacy:test_notion_webhook::job::test_redelivered_comment_event_creates_one_notification
    Scenario: An event that Notion delivers again keeps one notification, and does not bring a read one back
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text |
        | comment-1 | someone | 5      | @me  |
      When the Notion job handles the comment "comment-1" written by someone, visible to me only
      And the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is unread
      When I set that Notion notification as read
      And the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is read

    Scenario Outline: A new reply after the notification was deleted or unsubscribed
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text     |
        | comment-1 | someone | 5      | Ping @me |
      When the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my Notion notifications, whatever their status
      And I set that Notion notification as <dismissed>
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text    |
        | comment-1 | someone | 5      | Ping @me |
        | comment-2 | someone | 15     | <reply> |
      And the Notion job handles the comment "comment-2" written by someone, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is <after>

      @legacy:test_notion_webhook::job::test_new_reply_after_delete_or_unsubscribe::case_1_deleted
      Examples:
        | dismissed | reply     | after  |
        | deleted   | Any news? | unread |

      @legacy:test_notion_webhook::job::test_new_reply_after_delete_or_unsubscribe::case_2_unsubscribed
      Examples:
        | dismissed    | reply     | after        |
        | unsubscribed | Any news? | unsubscribed |

      @legacy:test_notion_webhook::job::test_new_reply_after_delete_or_unsubscribe::case_3_unsubscribed_then_mentioned
      Examples:
        | dismissed    | reply          | after  |
        | unsubscribed | Any news @me   | unread |

  Rule: Every connection of the workspace is handled on its own

    @legacy:test_notion_webhook::job::test_a_connection_without_access_does_not_stop_the_others
    Scenario: A connection that cannot see the page does not stop the others
      Given another user is signed in
      And the other user and I are connected to the same Notion workspace
      And Notion does not share the page with the token "notion-token-someone"
      And Notion serves, to the token "notion-token-me", a comment by a colleague that mentions someone and me
      When the Notion job handles the comment "comment-1" written by "a-colleague", visible to someone and me
      And I look at my unread Notion notifications
      Then my only Notion notification is unread
      When the other user looks at their Notion notifications, whatever their status
      Then I have 0 Notion notifications listed

    @legacy:test_notion_webhook::job::test_a_failing_connection_fails_the_job_after_saving_the_others
    Scenario: A Notion failure fails the job once the other connections are saved, and the retry saves the rest
      Given another user is signed in
      And the other user and I are connected to the same Notion workspace
      And Notion fails the comment requests of the token "notion-token-someone", after the access check
      And Notion serves, to the token "notion-token-someone", a comment by a colleague that mentions someone and me
      And Notion serves, to the token "notion-token-me", a comment by a colleague that mentions someone and me
      When the Notion worker runs the job of the comment "comment-1" written by "a-colleague", visible to someone and me
      Then the job fails, mentioning "Failed to save the Notion event for 1 connection(s)"
      When I look at my unread Notion notifications
      Then I have 1 Notion notification listed
      When the other user looks at their Notion notifications, whatever their status
      Then I have 0 Notion notifications listed
      When Notion answers again
      And Notion serves, to the token "notion-token-someone", a comment by a colleague that mentions someone and me
      And Notion serves, to the token "notion-token-me", a comment by a colleague that mentions someone and me
      And the Notion worker runs the same job again
      Then the job succeeds
      When I look at my unread Notion notifications
      Then I have 1 Notion notification listed
      When the other user looks at their Notion notifications, whatever their status
      Then I have 1 Notion notification listed

  Rule: Switching the comment sync off stops the discussions that the user already follows

    Scenario: A new comment in a discussion that I follow changes my notification, until I disable the comment sync
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text     |
        | comment-1 | someone | 5      | Ping @me |
      When the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my unread Notion notifications
      And I set that Notion notification as read
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text     |
        | comment-1 | someone | 5      | Ping @me |
        | comment-2 | someone | 15     | Any news |
      And the Notion job handles the comment "comment-2" written by someone, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is unread
      And that Notion notification was updated at "2026-10-10T09:15:00+00:00"

    Scenario: A new comment in a discussion that I follow leaves my notification alone once I disabled the comment sync
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text     |
        | comment-1 | someone | 5      | Ping @me |
      When the Notion job handles the comment "comment-1" written by someone, visible to me only
      And I look at my unread Notion notifications
      And I set that Notion notification as read
      And I disable the comment sync of my Notion connection
      And Notion now serves a discussion with these comments:
        | comment   | author  | minute | text     |
        | comment-1 | someone | 5      | Ping @me |
        | comment-2 | someone | 15     | Any news |
      And the Notion job handles the comment "comment-2" written by someone, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is read
      And that Notion notification was updated at "2026-10-10T09:05:00+00:00"

  Rule: An event of a workspace only concerns the connections of that workspace

    Scenario: A comment of another workspace, where only someone else is connected, creates nothing for me
      Given another user is signed in
      And I am connected to Notion, in a workspace of its own
      And the other user is connected to a Notion workspace of their own
      And Notion serves, to the token "notion-token-someone", a comment by a colleague that mentions someone and me
      And Notion serves, to the token "notion-token-me", a comment by a colleague that mentions someone and me
      When the Notion job handles the comment "comment-1" written by "a-colleague" in the workspace of the other user, visible to someone and me
      And I look at my Notion notifications, whatever their status
      Then I see no Notion notification
      When the Notion job handles the comment "comment-1" written by "a-colleague", visible to someone and me
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is unread
