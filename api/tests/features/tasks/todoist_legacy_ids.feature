Feature: Todoist legacy ids
  Todoist replaced its numeric ids with new ones. A task still holding an old id is
  fixed when Todoist rejects the old id, or by the migration command, which never
  touches a user whose ids Todoist refuses to map.

  Background:
    Given I am signed in
    And I am connected to Todoist
    And Todoist can list my projects

  Rule: A change on a task with an old id is retried with the new id

    @legacy:test_todoist_tasks::legacy_todoist_ids::test_patch_todoist_task_with_legacy_id_status_as_done
    Scenario: Completing a task with an old id completes it with the new id and keeps the new ids
      Given I have a Todoist task with the old id "9735649058"
      And Todoist rejects, with the error 557, the completion of the task "9735649058"
      And Todoist maps the old task id "9735649058" to the new id "6Jf8VQXxpwv56VQ7"
      And Todoist maps the old project id "2203306141" to the new id "6Jf8VQXxpwv56VQ9"
      And Todoist completes the task "6Jf8VQXxpwv56VQ7"
      When I mark that task as done
      Then the patched task is done
      And that task is done, with the Todoist ids "6Jf8VQXxpwv56VQ7" and "6Jf8VQXxpwv56VQ9"

    @legacy:test_todoist_tasks::legacy_todoist_ids::test_patch_todoist_task_with_legacy_id_status_and_title
    Scenario: Completing and renaming a task with an old id sends both changes with the new id, the rename once
      Given I have a Todoist task with the old id "9735649058"
      And Todoist rejects, with the error 557, the completion of the task "9735649058"
      And Todoist rejects, with the error 557, the rename of the task "9735649058" to "New title"
      And Todoist completes the task "6Jf8VQXxpwv56VQ7"
      And Todoist renames the task "6Jf8VQXxpwv56VQ7" to "New title", exactly once
      And Todoist maps the old task id "9735649058" to the new id "6Jf8VQXxpwv56VQ7"
      And Todoist maps the old project id "2203306141" to the new id "6Jf8VQXxpwv56VQ9"
      When I mark that task as done and rename it to "New title"
      Then the patched task is done
      And that task is done, with the Todoist ids "6Jf8VQXxpwv56VQ7" and "6Jf8VQXxpwv56VQ9"

    @legacy:test_todoist_tasks::legacy_todoist_ids::test_patch_todoist_task_with_unmapped_legacy_id_fails
    Scenario: An old id that Todoist cannot map fails the change and leaves the task as it was
      Given I have a Todoist task with the old id "9735649058"
      And Todoist rejects, with the error 557, the completion of the task "9735649058"
      And Todoist maps no old task id
      When I try to mark that task as done
      Then the app answers 500
      And that task is active, with the Todoist ids "9735649058" and "2203306141"

  Rule: The migration command replaces the old ids

    @legacy:test_todoist_tasks::legacy_todoist_ids::test_migrate_legacy_todoist_ids
    Scenario: The migration gives a task its new ids and deletes the task whose new id already exists
      Given I have a Todoist task with the old id "9735649058"
      And I have a Todoist task with the old id "9735649059"
      And I have a Todoist task with the new id "6Jf8VQXxpwv56VQ8"
      And Todoist maps the old task ids to the new ones, and the old project id to the new one
      When the legacy Todoist ids are migrated
      Then the task that had the id "9735649058" is active, with the Todoist ids "6Jf8VQXxpwv56VQ7" and "6Jf8VQXxpwv56VQ9"
      And the task that had the id "9735649059" is deleted, with the Todoist ids "9735649059" and "2203306141"
      And the task that had the id "6Jf8VQXxpwv56VQ8" is active, with the Todoist ids "6Jf8VQXxpwv56VQ8" and "6Jf8VQXxpwv56VQ9"
      And no legacy Todoist item is left for me

    @legacy:test_todoist_tasks::legacy_todoist_ids::test_migrate_legacy_todoist_ids_skips_user_on_todoist_error
    Scenario: A user whose ids Todoist refuses to map is left untouched
      Given I have a Todoist task with the old id "9735649058"
      And Todoist refuses the id mappings, as the access token was revoked
      When the legacy Todoist ids are migrated
      Then that task is active, with the Todoist ids "9735649058" and "2203306141"
