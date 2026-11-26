Feature: Choosing the task manager of a new task
  A user can be connected to several task managers. A task created from a notification goes
  to the one that the request names, else to the default one of the user's preferences. The
  preferences belong to their user.

  Background:
    Given I am signed in

  Rule: The default task manager is a preference of the user

    Scenario: The default task manager is not set at first, and can be set and changed
      When I read my preferences
      Then the app answers 200
      And the answer is my preferences, with no default task manager
      When I set my default task manager to TickTick
      Then the app answers 200
      And the answer is my preferences, with TickTick as my default task manager
      When I read my preferences
      Then the app answers 200
      And the answer is my preferences, with TickTick as my default task manager
      When I set my default task manager to Todoist
      Then the app answers 200
      And the answer is my preferences, with Todoist as my default task manager

    Scenario: My default task manager is not the one of another user, and theirs is not mine
      Given another user is signed in
      When I set my default task manager to TickTick
      And that other user reads their preferences
      Then the answer is the preferences of that other user, with no default task manager
      When that other user sets their default task manager to Todoist
      And that other user reads their preferences
      Then the answer is the preferences of that other user, with Todoist as their default task manager
      When I read my preferences
      Then the answer is my preferences, with TickTick as my default task manager

  Rule: A task created from a notification goes to the chosen task manager

    Background:
      Given I am connected to TickTick, syncing its tasks
      And I am connected to GitHub
      And I have a GitHub notification
      And GitHub accepts to mark that notification as done

    Scenario: A request that names TickTick creates the task in TickTick, although Todoist is connected too
      Given I am connected to Todoist
      And Todoist is not asked anything more
      And TickTick creates the item in the project Project2, as the Universal Inbox item, and then serves it
      When I create a TickTick task from that notification, in the project Project2
      Then the task that I created is the TickTick item in the project Project2, and its notification is deleted and was read just now
      And the notification stays linked to the TickTick task that I created
      And the TickTick task that I created belongs to my TickTick connection

    Scenario: A request without detail creates the task in TickTick when it is my default task manager
      Given I am connected to Todoist
      And Todoist is not asked anything more
      And TickTick is my default task manager
      And TickTick creates, without a project and without a priority, an item titled as that notification, and then serves it
      When I create a task from that notification, without any detail
      Then the task that I created without detail belongs to my TickTick connection, and that notification, read again, is deleted and linked to it

    Scenario: The default task manager of another user does not send my task to TickTick
      Given I am connected to Todoist, with the default project Project2, the due date today and the priority of the Todoist item
      And Todoist can list my projects
      And I set my default task manager to Todoist
      And another user is signed in
      And that other user set their default task manager to TickTick
      And Todoist adds the item with the defaults of my connection, as the title of that notification, and then serves it
      And TickTick is not asked anything
      When I create a task from that notification, without any detail
      Then the task that I created is the Todoist item in the project Project2, and its notification is deleted and was read just now
      And the notification stays linked to the task that I created

    Scenario: Without a named task manager and without a default one, the task goes to Todoist
      Given I am connected to Todoist, with the default project Project2, the due date today and the priority of the Todoist item
      And Todoist can list my projects
      And Todoist adds the item with the defaults of my connection, as the title of that notification, and then serves it
      And TickTick is not asked anything
      When I create a task from that notification, without any detail
      Then the task that I created is the Todoist item in the project Project2, and its notification is deleted and was read just now
      And the notification stays linked to the task that I created

    Scenario: With preferences that name no default task manager, the task goes to Todoist too
      Given I am connected to Todoist, with the default project Project2, the due date today and the priority of the Todoist item
      And Todoist can list my projects
      And I have preferences of my own, opening links in the background, without a default task manager
      And Todoist adds the item with the defaults of my connection, as the title of that notification, and then serves it
      And TickTick is not asked anything
      When I create a task from that notification, without any detail
      Then the task that I created is the Todoist item in the project Project2, and its notification is deleted and was read just now
      And the notification stays linked to the task that I created
