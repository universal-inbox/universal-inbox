Feature: The address of a Google Mail connection
  The address cached in a connection follows the Google account the connection is
  pinned to: it is read again from Gmail once the connection has been reconnected
  with another account, and reused otherwise.

  Background:
    Given I am signed in

  Scenario Outline: A connection cached the address "Previous@example.com" and is <pin>
    Given I am connected to Google Mail, which cached the address "Previous@example.com" and is <pin>
    And Google Mail reports no thread
    When I sync my Google Mail notifications
    Then Google Mail <profile>
    And my Google Mail connection has the address "<address>"

    @legacy:test_sync_google_mail_threads::test_sync_notifications_should_refresh_user_email_address_of_pinned_account::case_1_reconnected_with_another_account
    Examples:
      | pin | profile | address |
      | pinned to the Google account of "user@example.com"   | was asked for my profile once | user@example.com |

    @legacy:test_sync_google_mail_threads::test_sync_notifications_should_refresh_user_email_address_of_pinned_account::case_2_same_account
    Examples:
      | pin | profile | address |
      | pinned to the Google account of "previous@example.com" | was never asked for my profile | Previous@example.com |

    @legacy:test_sync_google_mail_threads::test_sync_notifications_should_refresh_user_email_address_of_pinned_account::case_3_not_pinned_yet
    Examples:
      | pin | profile | address |
      | not pinned to a Google account yet | was never asked for my profile | Previous@example.com |
