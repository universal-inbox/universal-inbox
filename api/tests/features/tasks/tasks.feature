Feature: The tasks
  A signed-in user reads, lists, searches and changes their tasks and finds
  the projects of their task provider. Nobody sees or changes the tasks of
  another user, and an invalid change is refused before the task is looked up.

  Background:
    Given I am signed in

  Rule: Reading one task

    Scenario: A task that does not exist is not found
      When I get a task that does not exist
      Then the app answers 404
      And the error message is "Cannot find task <id>", with <id> being the id of that unknown task

    Scenario: A user cannot read the task of another user
      Given I am connected to Todoist
      And Todoist can list my projects
      And a Todoist task called "mine"
      And another user is signed in
      When that other user gets the task "mine"
      Then the app answers 403
      And the error message is "Forbidden access: Only the owner of the task <id> can access it", with <id> being the id of the task "mine"

  Rule: Listing tasks

    Scenario: A user without tasks sees none
      When I list my active tasks
      Then I see no tasks

    Scenario: Tasks are listed by status and never to another user
      Given I am connected to Todoist
      And Todoist can list my projects
      And a Todoist task called "active one"
      And I have a done Todoist task "done one"
      And another user is signed in
      When I list my active tasks
      Then I see only the task "active one"
      When I list my done tasks
      Then I see only the task "done one"
      When that other user lists their done tasks
      Then I see no tasks

  Rule: Changing one task

    Scenario: A change that changes nothing is not modified
      Given I am connected to Todoist
      And Todoist can list my projects
      And a Todoist task called "mine"
      When I send "mine" the status it already has
      Then the app answers 304

    Scenario: A user cannot change the task of another user
      Given I am connected to Todoist
      And Todoist can list my projects
      And a Todoist task called "mine"
      And another user is signed in
      When that other user marks "mine" as done
      Then the app answers 403

    Scenario: A task cannot point at the stored item of another user
      Given Todoist can list my projects
      And I am connected to Todoist
      And a Todoist task called "mine"
      And another user is signed in
      And that other user is connected to Todoist
      And that other user has a Todoist task "victim"
      When I point "mine" at the stored item of "victim"
      Then the app answers 403
      When I point "mine" at the stored item of "mine"
      Then the app does not answer 403

    Scenario: A change without any value is refused
      Given I am connected to Todoist
      And Todoist can list my projects
      And a Todoist task called "mine"
      When I send the task "mine" a patch with nothing to change
      Then the app answers 400
      And the error message is "Invalid input data: Missing `status` field value to update task <id>", with <id> being the id of the task "mine"

    Scenario: A task that does not exist cannot be changed
      When I mark a task that does not exist as active
      Then the app answers 404
      And the error message is "Cannot update unknown task <id>", with <id> being the id of that unknown task

  Rule: An invalid change is refused before the task is looked up

    Scenario Outline: A patch with <field> <problem> is refused
      When I send a patch with <field> <problem> to a task that does not exist
      Then the app answers 400

      Examples:
        | field        | problem  |
        | title        | empty    |
        | title        | too-long |
        | body         | too-long |
        | project-name | too-long |

    Scenario: A patch with a field that does not exist is refused
      When I send a patch with the unknown field "user_id" to a task that does not exist
      Then the app answers 400

  Rule: Searching tasks

    Scenario: Searching with nothing to find returns nothing
      When I search my tasks for ""
      Then I find no task

    Scenario: Tasks are found by title and tags, not by description, and never for another user
      Given I am connected to Todoist
      And Todoist can list my projects
      And a Todoist task called "release"
      And a Todoist task called "other" titled "Other todo", with the description "fill the form"
      And another user is signed in
      Then the title of "release" is "Release new version of Universal Inbox"
      When I search my tasks for "Release new version"
      Then I find only the task "release"
      When I search my tasks for "todo"
      Then I find only the task "other"
      When I search my tasks for "form"
      Then I find no task
      When I search my tasks for "Food"
      Then I find exactly these tasks, in any order:
        | release |
        | other   |
      When that other user searches their tasks for "Task"
      Then I find no task

    Scenario: Tasks are searched across providers or in one of them
      Given I am connected to Todoist
      And Todoist can list my projects
      And I am connected to TickTick
      And a Todoist task called "todoist"
      And I have a TickTick task "ticktick" titled "Universal ticktick task"
      When I search my tasks for "Universal"
      Then I find 2 tasks
      When I search my Todoist tasks for "Universal"
      Then the task I find is titled "Release new version of Universal Inbox"
      When I search my TickTick tasks for "Universal"
      Then the task I find is titled "Universal ticktick task"

  Rule: Searching projects

    Scenario: Projects are found by part of their name
      Given I am connected to Todoist
      And Todoist can list my projects
      When I search my projects for "in"
      Then I find only the project "Inbox" with the id "1111"
      When I search my projects for "box"
      Then I find only the project "Inbox" with the id "1111"
      When I search my projects for "jec"
      Then I find only the project "Project2" with the id "2222"
