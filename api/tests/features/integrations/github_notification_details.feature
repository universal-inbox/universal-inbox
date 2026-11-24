Feature: Details of synced GitHub notifications
  Syncing a pull request or a discussion notification fetches its details, all the
  pages of its comments, reviews and replies, and drops the minimized ones. A
  failure while fetching them fails the sync and leaves the inbox as it was.

  Background:
    Given I am signed in
    And I am connected to GitHub

  Rule: Pull requests

    @legacy:test_sync_github_notifications::test_sync_pull_request_notification_with_comments_and_review_threads
    Scenario: A pull request notification holds all the pages of its comments and review threads
      Given GitHub reports one notification, on pull request 123
      And GitHub knows pull request 123, with comments, reviews and review threads over several pages
      When I sync my GitHub notifications
      Then I have one unread GitHub notification, whose pull request holds these comments, minimized ones dropped:
        | id   | body                  |
        | IC_1 | <p>first comment</p>  |
        | IC_3 | <p>a late comment</p> |
      And that pull request holds one review, PRR_1, saying "<p>Looks good overall</p>" and submitted at 2014-11-07T22:03:00Z
      And that pull request holds these review threads, with all their comment pages:
        | thread | comments |
        | PRRT_1 | RC_1, RC_2 |
        | PRRT_2 | RC_3       |
      And its first review thread is on lib/octokit/authenticator.rb, lines 13 to 15, starts with the hunk "@@ -10,6 +10,8 @@" and is not resolved
      And its second review thread is resolved and outdated, and falls back to its original line 20

  Rule: Discussions

    @legacy:test_sync_github_notifications::test_sync_discussion_notification_with_details
    Scenario: A discussion notification holds all the pages of its comments and replies
      Given GitHub reports one notification, on the discussion 123 titled "test discussion"
      And GitHub knows the discussion 123, with comments and replies over several pages
      When I sync my GitHub notifications
      Then I have one unread GitHub notification, whose discussion is "test discussion" at "https://github.com/octocat/universal-inbox/discussions/1", in the answerable category "Q&A", emoji ":pray:", slug "q-a"
      And that discussion holds the comments "DC_1, DC_4, DC_6"
      And its first comment says "<p>first comment</p>", is not the answer, has 3 replies and holds the replies "<p>a reply</p>, <p>a late reply</p>"
      And its second comment is the answer and holds no reply

    @legacy:test_sync_github_notifications::test_sync_discussion_notification_stops_on_non_advancing_cursor
    Scenario: A page of comments that points back to a page already read stops the paging
      Given GitHub reports one notification, on the discussion 123 titled "test discussion"
      And GitHub knows the discussion 123, whose second page of comments is the first one again
      When I sync my GitHub notifications
      Then I have one unread GitHub notification, whose discussion is "test discussion" at "https://github.com/octocat/universal-inbox/discussions/1", in the answerable category "Q&A", emoji ":pray:", slug "q-a"
      And that discussion holds the comments "DC_1, DC_4, DC_1, DC_4"

    @legacy:test_sync_github_notifications::test_sync_discussion_notification_with_error
    Scenario: A discussion GitHub fails to give fails the sync and leaves the inbox as it was
      Given GitHub reports one notification, on the discussion 123 titled "test discussion"
      And GitHub fails to give the discussion 123 with the error "Something went wrong"
      When I ask for a synchronous sync of my GitHub notifications, which fails
      Then the app answers 500
      And I have no unread notification
      And my GitHub connection recorded its first failed sync, and is still validated
