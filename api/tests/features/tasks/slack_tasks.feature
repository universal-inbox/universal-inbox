Feature: Slack tasks mirrored by Todoist
  A Slack reaction makes a task that Todoist mirrors. A sync of Todoist moves the task
  with its item, and Slack follows the task: its reaction is swapped when the task is
  done, and an edit of the message never overwrites or pushes the title of the task.

  Background:
    Given I am signed in

  Rule: A sync of Todoist moves the task mirrored from a Slack reaction

    Background:
      Given I am connected to Slack, syncing its tasks
      And I am connected to Todoist
      And Todoist can list my projects
      And Slack serves the message of the reaction events, with its author, channel, team and user groups

    Scenario Outline: A task made when the reaction was <event>, then <after> in the Inbox of Todoist
      Given Todoist adds the Slack message as an item of the Inbox, and then serves it as <before>
      When Slack tells that the reaction "eyes" was <event> on that message
      Then that Slack task is <before>
      Given Todoist then reports that item as <after>, in the project "1111"
      And Slack <reaction_call> the reaction "eyes" on that message, once
      When I sync the tasks of my Todoist connection
      Then that Slack task, read again, is <after>
      And the sync brought two tasks, each with one notification
      And the notification of that Slack task is <notification>

      @legacy:test_slack_tasks::test_sync_todoist_slack_task::case_1_with_completed_task_in_inbox
      Examples:
        | event | before | after | notification | reaction_call |
        | added | active | done  | deleted      | removes       |

      @legacy:test_slack_tasks::test_sync_todoist_slack_task::case_3_with_uncompleted_task_in_inbox
      Examples:
        | event   | before | after  | notification | reaction_call |
        | removed | done   | active | unread       | adds          |

    Scenario Outline: A task made when the reaction was <event>, then <after> in another project of Todoist
      Given Todoist adds the Slack message as an item of the Inbox, and then serves it as <before>
      When Slack tells that the reaction "eyes" was <event> on that message
      Then that Slack task is <before>
      Given Todoist then reports that item as <after>, in the project "2222"
      And Slack <reaction_call> the reaction "eyes" on that message, once
      When I sync the tasks of my Todoist connection
      Then that Slack task, read again, is <after>
      And the sync brought two tasks
      And that Slack task has no notification

      @legacy:test_slack_tasks::test_sync_todoist_slack_task::case_2_with_completed_task_not_in_inbox
      Examples:
        | event | before | after | reaction_call |
        | added | active | done  | removes       |

      @legacy:test_slack_tasks::test_sync_todoist_slack_task::case_4_with_uncompleted_task_not_in_inbox
      Examples:
        | event   | before | after  | reaction_call |
        | removed | done   | active | adds          |

  Rule: Completing or editing a Slack task

    Background:
      Given I am connected to Slack, syncing its tasks
      And I am connected to Todoist
      And Todoist can list my projects
      And Todoist adds the Slack message as an item, and then serves it
      And I reacted to a Slack message, which Todoist mirrors as a task

    @legacy:test_slack_tasks::test_slack_task_should_not_overwrite_nor_push_the_title_of_a_mirrored_task
    Scenario: Editing the Slack message neither overwrites the title of the task nor pushes anything to Todoist
      Given Todoist is not asked to change anything
      When that Slack message is edited upstream, with an attachment titled "Edited upstream in Slack"
      Then the mirrored task is the same task, still titled as before, without "Edited upstream in Slack"

    Scenario Outline: Completing the task completes its item in Todoist and removes the reaction in Slack, <slack_case>
      Given Todoist completes the item that mirrors that Slack task
      And <slack_answer>
      When I mark that Slack task as done
      Then that task is done, with a completion date, and nothing else changed

      @legacy:test_slack_tasks::test_patch_slack_task_status_as_done::case_1_reaction_removed
      Examples:
        | slack_case                     | slack_answer                                                                      |
        | as Slack removes the reaction  | Slack removes the reaction "eyes" from that message, once                         |

      @legacy:test_slack_tasks::test_patch_slack_task_status_as_done::case_2_reaction_already_removed
      Examples:
        | slack_case                          | slack_answer                                                                                   |
        | though the reaction is already gone | Slack answers, once, that the reaction "eyes" is already removed from that message              |

  Rule: A Slack task completed with a reaction of its own

    @legacy:test_slack_tasks::test_patch_slack_task_status_as_done_with_completion_reaction
    Scenario: Completing the task swaps the reaction of the message for the completion reaction
      Given I am connected to Slack, syncing its tasks, and marking the completed ones with the reaction "done"
      And I am connected to Todoist
      And Todoist can list my projects
      And Todoist adds the Slack message as an item, and then serves it
      And I reacted to a Slack message, which Todoist mirrors as a task
      And Todoist completes the item that mirrors that Slack task
      And Slack removes the reaction "eyes" on that message, once
      And Slack adds the reaction "done" on that message, once
      When I mark that Slack task as done
      Then that task is done, with a completion date, and nothing else changed
