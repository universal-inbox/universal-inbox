Feature: Syncing Notion comments
  The hourly sync reads again the discussions it already tracks, and notifies the
  replies the webhook missed. A discussion without new reply is left alone, and a
  discussion inactive for two weeks is not read again.

  Background:
    Given I am signed in

  Rule: Tracked discussions are read again when the connection syncs comments

    Background:
      Given I am connected to Notion, syncing only the comments of my discussions

    Scenario: A missed reply reopens the discussion
      Given I already track the Notion discussion first, whose first comment was written 120 min ago, and I have read it
      And Notion's discussion first holds the comment "Looks good", written by someone 120 min ago
      And Notion's discussion first holds the comment "Any news?", written by someone 10 min ago
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then I have 1 Notion notification, all unread
      And it holds 2 comments

    Scenario Outline: A missed reply in a discussion I unsubscribed from, <mention>
      Given I already track the Notion discussion first, whose first comment was written 120 min ago, and I have read it
      And I unsubscribe from my notification of the Notion discussion first
      And Notion's discussion first holds the comment "Looks good", written by someone 120 min ago
      And Notion's discussion first holds a reply "Any news ", written by someone 10 min ago, <mention>
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then I have 1 Notion notification, all <status>

      Examples:
        | mention            | status       |
        | not mentioning me  | unsubscribed |

      Examples:
        | mention        | status |
        | mentioning me  | unread |

    Scenario: The missed replies of two discussions of one page read the page once
      Given I already track the Notion discussion first, whose first comment was written 120 min ago, and I have read it
      And I already track the Notion discussion second, whose first comment was written 120 min ago, and I have read it
      And Notion's discussion first holds the comment "Looks good", written by someone 120 min ago
      And Notion's discussion second holds the comment "Typo here", written by someone 120 min ago
      And Notion's discussion first holds the comment "Any news?", written by someone 10 min ago
      And Notion's discussion second holds the comment "Fixed?", written by someone 10 min ago
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then I have 2 Notion notifications, all unread
      And Notion was asked for the page and its users once each

    Scenario: A discussion without new reply is left untouched
      Given I already track the Notion discussion first, whose first comment was written 120 min ago, and I have read it
      And Notion's discussion first holds the comment "Looks good", written by someone 120 min ago
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then I have 1 Notion notification, all read
      And Notion's comments were read

    Scenario: A discussion is read again at most once an hour
      Given I already track the Notion discussion first, whose first comment was written 120 min ago, and I have read it
      And Notion's discussion first holds the comment "Looks good", written by someone 120 min ago
      And I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      And Notion's comments were read
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then Notion's comments were not read by the last sync

    Scenario: A discussion inactive for two weeks is not read again
      Given I already track the Notion discussion first, whose first comment was written 15 days ago, and I have read it
      And Notion's discussion first holds the comment "Looks good", written by someone 15 days ago
      And Notion's discussion first holds the comment "Any news?", written by someone 10 min ago
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then Notion's comments were not read by the last sync
      And I have 1 Notion notification, all read

  Rule: Tracked discussions are not read again when the connection does not sync comments

    Scenario: A connection syncing only mentions does not read the discussions again
      Given I am connected to Notion, syncing only my mentions
      And I already track the Notion discussion first, whose first comment was written 120 min ago, and I have read it
      And Notion has the page "Roadmap", edited 1 min ago
      When I force a sync of my Notion notifications, ignoring the minimum interval between syncs
      Then Notion's comments were not read by the last sync
