Feature: Dead jobs
  A job that failed all its attempts is dead. Dead jobs are listed, most
  recently dead first, including those that can no longer be decoded. A dead
  job is requeued to run again, or deleted for good.

  Rule: Dead jobs are listed

    Scenario: Dead jobs are listed from the most recently dead, with what they failed on
      Given a dead "purge" job "older purge" that died 2 hours ago
      And a dead "purge" job "purge" that died 1 hour ago
      And a dead "sync tasks" job "sync" that died just now
      When I list the dead jobs
      Then the listed jobs are, most recently dead first:
        | sync        |
        | purge       |
        | older purge |
      And the listed job "sync" is a decodable "SyncTasks" job of that user, which ran 3 attempts, died at the reference time and failed with "Max retries of 3 exceeded"
      And the listed job "purge" belongs to no user
      When I list at most 1 dead job of the type "PurgeAuditEvents"
      Then the only listed job is "purge"

    Scenario: Dead jobs that cannot be decoded are listed too and can be filtered
      Given a dead "sync tasks" job "sync" that died 1 hour ago
      And a dead job "raw" whose payload is not JSON, that died just now
      And a dead job "removed" of a type that no longer exists, that died 2 hours ago
      When I list the dead jobs
      Then the listed jobs are, most recently dead first:
        | raw     |
        | sync    |
        | removed |
      And the listed job "raw" has the undecodable type and cannot be decoded
      And the listed job "removed" has the type "RemovedJob (undecodable)" and 3 attempts
      When I list the undecodable dead jobs
      Then the listed jobs are, most recently dead first:
        | raw     |
        | removed |

  Rule: A dead job is requeued to run again

    Scenario: A requeued job is active again and its workers are woken up
      Given a dead "sync tasks" job "sync" that died just now
      When I requeue the dead job "sync"
      Then the job is requeued
      And "sync" is no longer dead and is the only active job
      And the workers were woken up once
      And a worker reads "sync" as a "SyncTasks" job that has not run yet
      And the queue has 1 pending and 0 dead jobs
      When I requeue the dead job "sync"
      Then the app says the job is not dead
      And "sync" is still the only active job

    Scenario: A job that cannot be decoded is not requeued
      Given a dead job "removed" of a type that no longer exists, that died just now
      When I requeue the dead job "removed"
      Then the requeue outcome is "undecodable"
      And "removed" is still dead and nothing is active

  Rule: A dead job is deleted for good

    Scenario: Deleting a dead job by its id removes its data and its result and nothing else
      Given a dead "purge" job "deleted" that died just now
      And a dead "purge" job "kept" that died just now
      When I select the dead jobs with the id of "deleted" and an id nobody knows
      Then the only selected job is "deleted"
      When I delete the selected jobs
      Then "deleted" is no longer dead
      And neither the data nor the result of "deleted" is stored anymore
      And "kept" is still dead
      And deleting "deleted" again finds nothing to delete

    Scenario: Dead jobs older than a week are selected and deleted, recent ones are kept
      Given a dead "purge" job "old" that died 8 days ago
      And a dead "purge" job "recent" that died 6 days ago
      When I select the dead jobs that died more than 7 days ago
      Then the only selected job is "old"
      When I delete the selected jobs
      Then "old" is no longer dead
      And "recent" is still dead

    Scenario: A dead job that cannot be decoded is deleted by its id
      Given a dead job "raw" whose payload is not JSON, that died just now
      When I select the undecodable dead jobs with the id of "raw"
      Then the only selected job is "raw"
      When I delete the selected jobs
      Then "raw" is no longer dead
      And the data of "raw" is not stored anymore
