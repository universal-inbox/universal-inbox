Feature: Syncing Notion mentions
  The hourly sync searches the pages edited since the connection was created and
  notifies the mentions the webhook missed, once each. A mention that disappears
  from its page deletes its notification.

  Background:
    Given I am signed in

  Rule: Mentions are searched when the connection syncs them

    Background:
      Given I am connected to Notion, syncing only my mentions

    @legacy:test_sync_notion_notifications::mentions::test_sync_creates_a_missed_mention
    Scenario: A missed mention becomes a notification
      Given Notion has the page "Roadmap", edited 1 min ago
      And that page holds a paragraph mentioning me, written by someone 1 min ago
      When I sync my Notion notifications
      Then I have 1 Notion notification, all unread
      And my Notion notification is titled "Mentioned by User notion-user-someone on Roadmap"
      And it is a mention of me in a block of the page "Roadmap", saying "Can you check @Me"
      And my Notion connection recorded a completed sync

    @legacy:test_sync_notion_notifications::mentions::test_webhook_mention_is_not_duplicated_by_the_sync
    Scenario: A mention the webhook already reported is not notified a second time
      Given Notion has the page "Roadmap", edited 1 min ago
      And that page holds a paragraph mentioning me, written by someone 1 min ago
      And the Notion webhook reported that mention before the sync
      And I have 1 Notion notification
      And I mark my Notion notifications as read
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then I have 1 Notion notification, all read

    @legacy:test_sync_notion_notifications::mentions::test_known_mention_is_not_notified_again
    Scenario: A known mention is not notified again
      Given Notion has the page "Roadmap", edited 1 min ago
      And that page holds a paragraph mentioning me, written by someone 1 min ago
      And I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      And I mark my Notion notifications as read
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then I have 1 Notion notification, all read

    @legacy:test_sync_notion_notifications::mentions::test_known_mentions_of_one_page_are_not_notified_again
    Scenario: Each known mention of a page keeps its own notification
      Given Notion has the page "Roadmap", edited 1 min ago
      And that page holds a paragraph mentioning me, written by someone 1 min ago
      And that page holds a second paragraph mentioning me, written by someone 1 min ago
      And I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      And I have 2 Notion notifications
      And I mark my Notion notifications as read
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then I have 2 Notion notifications, all read

    @legacy:test_sync_notion_notifications::mentions::test_removed_mention_deletes_the_notification
    Scenario: A mention removed from its page deletes the notification
      Given Notion has the page "Roadmap", edited 1 min ago
      And that page holds a paragraph mentioning me, written by someone 1 min ago
      And I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      And Notion has the page "Roadmap", edited 0 min ago
      And that page holds a paragraph saying "Never mind", written by someone 0 min ago
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then I have 1 Notion notification, all deleted

    @legacy:test_sync_notion_notifications::mentions::test_block_moved_to_the_trash_deletes_the_notification
    Scenario: A block moved to the trash deletes the notification
      Given Notion has the page "Roadmap", edited 1 min ago
      And that page holds a paragraph mentioning me, written by someone 1 min ago
      And I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      And I mark my Notion notifications as read
      And Notion has the page "Roadmap", edited 0 min ago
      And that page holds the paragraph mentioning me, written by someone 0 min ago, moved to the trash
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then I have 1 Notion notification, all deleted

    @legacy:test_sync_notion_notifications::mentions::test_mention_written_by_me_is_ignored
    Scenario: A mention I wrote myself is ignored
      Given Notion has the page "Roadmap", edited 1 min ago
      And that page holds a paragraph mentioning me, written by me 1 min ago
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then I have no Notion notification

    @legacy:test_sync_notion_notifications::mentions::test_block_edited_before_the_cursor_is_ignored
    Scenario: A block edited before the connection was created is not a missed mention
      Given Notion has the page "Roadmap", edited 1 min ago
      And that page holds a paragraph mentioning me, written by someone 60 min ago
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then I have no Notion notification

    @legacy:test_sync_notion_notifications::mentions::test_search_stops_at_the_first_page_older_than_the_cursor
    Scenario: The search stops at the first page older than the connection
      Given Notion has the page "Roadmap", edited 1 min ago
      And Notion lists an older page, edited 60 min ago, after that page, with more pages to come
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then Notion was searched 1 time
      And Notion was asked for the content of the older page 0 times
      And Notion was asked for the content of Roadmap 1 time

  Rule: Mentions are not searched when the connection does not sync them

    @legacy:test_sync_notion_notifications::mentions::test_disabled_mentions_are_not_searched
    Scenario: A connection syncing only comments does not search for mentions
      Given I am connected to Notion, syncing only the comments of my discussions
      And Notion has the page "Roadmap", edited 1 min ago
      And that page holds a paragraph mentioning me, written by someone 1 min ago
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then Notion was searched 0 times
      And I have no Notion notification
