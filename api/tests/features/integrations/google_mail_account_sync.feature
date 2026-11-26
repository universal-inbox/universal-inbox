Feature: The address of a Google Mail connection
  The address cached in a connection follows the Google account the connection is
  pinned to: it is read again from Gmail once the connection has been reconnected
  with another account, and reused otherwise.

  Background:
    Given I am signed in

  Scenario: A connection reconnected with another Google account reads its address again
    Given I am connected to Google Mail, which cached the address "Previous@example.com" and is pinned to the Google account of "user@example.com"
    And Google Mail reports no thread
    When I sync my Google Mail notifications
    Then my sync returned no notification
    And the number of profile requests Google Mail received is 1
    And my Google Mail connection has the address "user@example.com"

  Scenario: A connection still pinned to the same Google account keeps its cached address
    Given I am connected to Google Mail, which cached the address "Previous@example.com" and is pinned to the Google account of "previous@example.com"
    And Google Mail reports no thread
    When I sync my Google Mail notifications
    Then my sync returned no notification
    And the number of profile requests Google Mail received is 0
    And my Google Mail connection has the address "Previous@example.com"

  Scenario: A connection not pinned to a Google account yet keeps its cached address
    Given I am connected to Google Mail, which cached the address "Previous@example.com" and is not pinned to a Google account yet
    And Google Mail reports no thread
    When I sync my Google Mail notifications
    Then my sync returned no notification
    And the number of profile requests Google Mail received is 0
    And my Google Mail connection has the address "Previous@example.com"
