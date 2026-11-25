Feature: Handling a Notion page content event
  The job that handles the update of the blocks of a page decides, for each user connected to the workspace
  who can see the page, whether a block mentions them, and creates, keeps, deletes or restores their notification.

  Background:
    Given I am signed in

  Rule: A block that mentions me creates a notification

    @legacy:test_notion_webhook::mention::test_mention_creates_an_unread_notification
    Scenario: A block that mentions me creates an unread notification
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a block last edited by someone at minute 5, reading "Can you check ", with a mention of me
      When the Notion job handles an update of the block, visible to me only
      And I look at my unread Notion notifications
      Then my only Notion notification is unread
      And that Notion notification is titled "Mentioned by <someone> on Roadmap"
      And that Notion notification was updated at "2026-10-10T09:05:00+00:00"
      And that Notion notification is the block of the page, reading "Can you check @Me"

    @legacy:test_notion_webhook::mention::test_mention_in_a_nested_table_row_creates_a_notification
    Scenario: A mention in a table row, itself under a table block, creates a notification
      Given I am connected to Notion, in a workspace of its own
      And Notion serves a table whose row, last edited by someone, holds "Owner" and a mention of me
      When the Notion job handles an update of the block, visible to me only
      And I look at my unread Notion notifications
      Then my only Notion notification is unread
      And that Notion notification is a table row of the page, reading "Owner | @Me"

    @legacy:test_notion_webhook::mention::test_repeated_block_creates_one_notification
    Scenario: A block named twice, then again, keeps one notification
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a block last edited by someone at minute 5, reading "Can you check ", with a mention of me
      When the Notion job handles an update that names the block twice, visible to me only
      And the Notion job handles an update of the block, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is unread

  Rule: Nothing is created when the mention does not concern me

    @legacy:test_notion_webhook::mention::test_mentioning_myself_creates_nothing
    Scenario: A block that I edited myself creates nothing
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a block last edited by me at minute 5, reading "Can you check ", with a mention of me
      When the Notion job handles an update of the block, visible to me only
      And I look at my Notion notifications, whatever their status
      Then I see no Notion notification

    @legacy:test_notion_webhook::mention::test_disabled_mention_sync_creates_nothing
    Scenario: With the mention sync disabled, a block creates nothing
      Given I am connected to Notion, with its mention sync disabled
      And Notion now serves a block last edited by someone at minute 5, reading "Can you check ", with a mention of me
      When the Notion job handles an update of the block, visible to me only
      And I look at my Notion notifications, whatever their status
      Then I see no Notion notification

    @legacy:test_notion_webhook::mention::test_page_not_accessible_to_me_creates_nothing
    Scenario: A page that I cannot see creates nothing
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a block last edited by someone at minute 5, reading "Can you check ", with a mention of me
      When the Notion job handles an update of the block, visible to someone only
      And I look at my Notion notifications, whatever their status
      Then I see no Notion notification

  Rule: The notification follows the block

    @legacy:test_notion_webhook::mention::test_edit_keeping_the_mention_leaves_the_notification_as_is
    Scenario: An edit that keeps the mention leaves the status of the notification, and updates its text
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a block last edited by someone at minute 5, reading "Can you check ", with a mention of me
      When the Notion job handles an update of the block, visible to me only
      And I look at my unread Notion notifications
      And I set that Notion notification as read
      And Notion now serves a block last edited by someone at minute 7, reading "Could you check ", with a mention of me
      And the Notion job handles an update of the block, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is read
      And that Notion notification is the block of the page, reading "Could you check @Me"

    @legacy:test_notion_webhook::mention::test_removed_mention_deletes_the_notification_and_a_new_one_restores_it
    Scenario: A removed mention deletes the notification, and a new mention restores it
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a block last edited by someone at minute 5, reading "Can you check ", with a mention of me
      When the Notion job handles an update of the block, visible to me only
      And Notion now serves a block last edited by someone at minute 7, reading "Never mind", without any mention
      And the Notion job handles an update of the block, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is deleted
      When Notion now serves a block last edited by someone at minute 9, reading "Actually, ", with a mention of me
      And the Notion job handles an update of the block, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is unread

    @legacy:test_notion_webhook::mention::test_deleted_block_deletes_the_notification
    Scenario: A deleted block deletes the notification
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a block last edited by someone at minute 5, reading "Can you check ", with a mention of me
      When the Notion job handles an update of the block, visible to me only
      And Notion now reports that block as deleted
      And the Notion job handles an update of the block, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is deleted

  Rule: Every connection is handled on its own, and an update is read within a limit

    @legacy:test_notion_webhook::mention::test_a_failing_connection_fails_the_job_after_saving_the_others
    Scenario: A Notion failure fails the job once the other connections are saved, and the retry saves the rest
      Given another user is signed in
      And the other user and I are connected to the same Notion workspace
      And Notion fails the block requests of the token "notion-token-someone", after the access check
      And Notion serves, to the token "notion-token-someone", a block by a colleague that mentions someone and me
      And Notion serves, to the token "notion-token-me", a block by a colleague that mentions someone and me
      When the Notion worker runs the job of an update of the block, visible to someone and me
      Then the job fails, mentioning "Failed to save the Notion event for 1 connection(s)"
      When I look at my unread Notion notifications
      Then I have 1 Notion notification listed
      When the other user looks at their Notion notifications, whatever their status
      Then I have 0 Notion notifications listed
      When Notion answers again
      And Notion serves, to the token "notion-token-someone", a block by a colleague that mentions someone and me
      And Notion serves, to the token "notion-token-me", a block by a colleague that mentions someone and me
      And the Notion worker runs the same job again
      Then the job succeeds
      When I look at my unread Notion notifications
      Then I have 1 Notion notification listed
      When the other user looks at their Notion notifications, whatever their status
      Then I have 1 Notion notification listed

    @legacy:test_notion_webhook::mention::test_only_the_first_blocks_of_a_big_update_are_read
    Scenario: Only the first blocks of a big update are read
      Given I am connected to Notion, in a workspace of its own
      And Notion serves 60 blocks without any mention
      When the Notion job handles an update of 60 blocks, visible to me only
      Then Notion was asked to read 50 blocks

  Rule: Switching the mention sync off stops the blocks that the user already follows

    Scenario: A new edit of a block that I follow changes my notification, until I disable the mention sync
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a block last edited by someone at minute 5, reading "Can you check ", with a mention of me
      When the Notion job handles an update of the block, visible to me only
      And I look at my unread Notion notifications
      And I set that Notion notification as read
      And Notion now serves a block last edited by someone at minute 7, reading "Could you check ", with a mention of me
      And the Notion job handles an update of the block, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is read
      And that Notion notification is the block of the page, reading "Could you check @Me"

    Scenario: A new edit of a block that I follow leaves my notification alone once I disabled the mention sync
      Given I am connected to Notion, in a workspace of its own
      And Notion now serves a block last edited by someone at minute 5, reading "Can you check ", with a mention of me
      When the Notion job handles an update of the block, visible to me only
      And I look at my unread Notion notifications
      And I set that Notion notification as read
      And I disable the mention sync of my Notion connection
      And Notion now serves a block last edited by someone at minute 7, reading "Could you check ", with a mention of me
      And the Notion job handles an update of the block, visible to me only
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is read
      And that Notion notification is the block of the page, reading "Can you check @Me"

  Rule: An event of a workspace only concerns the connections of that workspace

    Scenario: A block update of another workspace, where only someone else is connected, creates nothing for me
      Given another user is signed in
      And I am connected to Notion, in a workspace of its own
      And the other user is connected to a Notion workspace of their own
      And Notion serves, to the token "notion-token-someone", a block by a colleague that mentions someone and me
      And Notion serves, to the token "notion-token-me", a block by a colleague that mentions someone and me
      When the Notion job handles an update of the block in the workspace of the other user, visible to someone and me
      And I look at my Notion notifications, whatever their status
      Then I see no Notion notification
      When the Notion job handles an update of the block, visible to someone and me
      And I look at my Notion notifications, whatever their status
      Then my only Notion notification is unread
