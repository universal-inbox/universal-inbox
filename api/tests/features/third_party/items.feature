Feature: Third-party items
  A task item is created only for a given user, through their validated
  Todoist connection, and only by the app itself: no route accepts one. A web
  page sent as a notification item must be a valid page.

  Rule: Task items are created through the app only

    Scenario: A task item is created for the given user through their Todoist connection
      Given I am signed in
      And I am connected to Todoist
      And Todoist can list my projects
      When a Todoist task is stored for me
      Then the stored item is mine, comes from my Todoist connection and holds the data that was given, and a task was created

    Scenario: A task item needs a validated Todoist connection
      Given I am signed in
      When I try to store a Todoist task for myself
      Then the store is refused as an unsupported action saying "No validated Todoist integration connection"

    Scenario: A web page cannot become a task item
      Given I am signed in
      When I try to store a web page as a task for myself
      Then the store is refused as an unsupported action saying "Cannot create a task item"

    Scenario: No route creates a task item
      Given I am signed in
      When I send a Todoist item to "third_party/task/items"
      Then the app answers 404

  Rule: A web page sent as a notification item must be valid

    Scenario: A page whose url is not a web address is refused
      Given I am signed in
      When I send a web page with the url "javascript:alert(1)" to the notification items
      Then the app answers 400

    Scenario: A page whose favicon is not a web address is refused
      Given I am signed in
      When I send a web page with the favicon "data:image/png;base64,AAAA" to the notification items
      Then the app answers 400

    Scenario: A page whose title is too long is refused
      Given I am signed in
      When I send a web page whose title is one character too long to the notification items
      Then the app answers 400

  Rule: A web page sent by the browser extension becomes a notification

    Background:
      Given I am signed in

    Scenario: Each page that I send becomes an unread notification of mine, and is stored
      When I send the web page "https://www.universal-inbox.com", titled "Universal Inbox", to the notification items, as "first"
      Then "first" is an unread notification of mine, from the API, titled "Universal Inbox" and opening "https://www.universal-inbox.com"
      And "first", read again, is the notification that was answered
      When I send the web page "https://app.universal-inbox.com", titled "Universal Inbox app", to the notification items, as "second"
      Then "second" is an unread notification of mine, from the API, titled "Universal Inbox app" and opening "https://app.universal-inbox.com"
      And "second", read again, is the notification that was answered
      And "first" and "second" are two notifications, of two items

    Scenario: The same page sent by another user is their own notification, and they cannot read mine
      Given another user is signed in
      When I send the web page "https://www.universal-inbox.com", titled "Universal Inbox", to the notification items, as "mine"
      And that other user sends the web page "https://www.universal-inbox.com", titled "Universal Inbox", to the notification items, as "theirs"
      Then "theirs" belongs to that other user
      And "mine" and "theirs" are two notifications, of two items
      And "mine", read again, is the notification that was answered
      And "theirs", read again, is the notification that was answered
      When that other user tries to read "mine"
      Then the app answers 403
