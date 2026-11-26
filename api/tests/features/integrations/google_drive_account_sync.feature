Feature: The user of a Google Drive connection
  The address and name cached in a connection follow the Google account the
  connection is pinned to: they are read again from Drive once the connection has
  been reconnected with another account, and reused otherwise.

  Background:
    Given I am signed in
    And Google Drive knows me as "John Roe", with the address "John.Roe@example.com"
    And Google Drive lists no file

  Scenario: A connection reconnected with another Google account reads its user again
    Given I am connected to Google Drive, which cached me as "Previous User" and is pinned to the Google account of "john.roe@example.com"
    When I sync my Google Drive notifications
    Then the sync returned 0 notification
    And the number of user info requests Google Drive received is 1
    And my Google Drive connection knows me as "John Roe", with the address "John.Roe@example.com"

  Scenario: A connection still pinned to the same Google account keeps its cached user
    Given I am connected to Google Drive, which cached me as "Previous User" and is pinned to the Google account of "previous@example.com"
    When I sync my Google Drive notifications
    Then the sync returned 0 notification
    And the number of user info requests Google Drive received is 0
    And my Google Drive connection knows me as "Previous User", with the address "Previous@example.com"

  Scenario: A connection not pinned to a Google account yet keeps its cached user
    Given I am connected to Google Drive, which cached me as "Previous User" and is not pinned to a Google account yet
    When I sync my Google Drive notifications
    Then the sync returned 0 notification
    And the number of user info requests Google Drive received is 0
    And my Google Drive connection knows me as "Previous User", with the address "Previous@example.com"
