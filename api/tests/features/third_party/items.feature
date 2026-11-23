Feature: Third-party items
  A task item is created only for a given user, through their validated
  Todoist connection, and only by the app itself: no route accepts one. A web
  page sent as a notification item must be a valid page.

  Rule: Task items are created through the app only

    @legacy:test_third_party::test_create_task_third_party_item_uses_given_user
    Scenario: A task item is created for the given user through their Todoist connection
      Given I am signed in
      And I am connected to Todoist
      And Todoist can list my projects
      When a Todoist task is stored for me
      Then the stored item is mine, comes from my Todoist connection and holds the data that was given, and a task was created

    @legacy:test_third_party::test_create_task_third_party_item_requires_validated_integration_connection
    Scenario: A task item needs a validated Todoist connection
      Given I am signed in
      When I try to store a Todoist task for myself
      Then the store is refused as an unsupported action saying "No validated Todoist integration connection"

    @legacy:test_third_party::test_create_task_third_party_item_rejects_notification_only_kind
    Scenario: A web page cannot become a task item
      Given I am signed in
      When I try to store a web page as a task for myself
      Then the store is refused as an unsupported action saying "Cannot create a task item"

    @legacy:test_third_party::test_create_task_third_party_item_has_no_http_route
    Scenario: No route creates a task item
      Given I am signed in
      When I send a Todoist item to "third_party/task/items"
      Then the app answers 404

  Rule: A web page sent as a notification item must be valid

    @legacy:test_third_party::test_create_notification_third_party_item_rejects_invalid_web_page
    Scenario: A page whose url is not a web address is refused
      Given I am signed in
      When I send a web page with the url "javascript:alert(1)" to the notification items
      Then the app answers 400

    @legacy:test_third_party::test_create_notification_third_party_item_rejects_invalid_web_page
    Scenario: A page whose favicon is not a web address is refused
      Given I am signed in
      When I send a web page with the favicon "data:image/png;base64,AAAA" to the notification items
      Then the app answers 400

    @legacy:test_third_party::test_create_notification_third_party_item_rejects_invalid_web_page
    Scenario: A page whose title is too long is refused
      Given I am signed in
      When I send a web page whose title is one character too long to the notification items
      Then the app answers 400
