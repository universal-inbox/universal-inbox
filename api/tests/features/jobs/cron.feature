Feature: Scheduled jobs
  Scheduled jobs run on a cron tick. When several worker processes handle the
  same tick, the job runs once. The lock of a tick belongs to its job queue.
  The vacuum removes the completed jobs older than its retention, in batches.

  Rule: A tick runs once

    @legacy:test_cron::test_try_acquire_cron_tick_lock_dedupes_same_tick
    Scenario: Only the first process takes the lock of a tick
      When I try to take the lock of a cron tick twice, then the lock of the tick 5 minutes later
      Then the first try wins the lock, the second does not and the next tick gets its own lock

    @legacy:test_cron::test_refresh_oauth_tokens_cron_tick_enqueues_job_once
    Scenario: The OAuth token refresh is enqueued once per tick
      Given an empty job queue
      When the refresh OAuth tokens cron tick is handled by 2 worker processes, refreshing 42 minutes before expiry
      Then exactly 1 job was enqueued, although 2 processes handled the same tick

    @legacy:test_cron::test_pause_integration_connections_cron_tick_enqueues_job_once
    Scenario: The pause of inactive integration connections is enqueued once per tick
      Given an empty job queue
      When the pause integration connections cron tick is handled by 2 worker processes, with an inactivity threshold of 42 days
      Then exactly 1 job was enqueued, although 2 processes handled the same tick

    @legacy:test_cron::test_purge_audit_events_cron_tick_enqueues_job_once
    Scenario: The purge of audit events is enqueued once per tick
      Given an empty job queue
      When the purge audit events cron tick is handled by 2 worker processes
      Then exactly 1 job was enqueued, although 2 processes handled the same tick

    @legacy:test_cron::test_vacuum_jobs_cron_tick_dedupes_same_tick
    Scenario: The vacuum runs once per tick
      Given an empty job queue
      And the vacuum cron keeps the jobs of the last 6 hours, in batches of 1000 with at most 200 batches per tick
      And a completed job "old-job" that finished 7 hours ago
      When the vacuum cron tick is handled
      And a completed job "another-old-job" that finished 7 hours ago is added
      And the same vacuum cron tick is handled again by a second worker process
      Then the job "old-job" is purged from the done set, the data and the result
      And the job "another-old-job" is kept in the done set, the data and the result

  Rule: The lock of a tick belongs to its job queue

    @legacy:test_cron::test_vacuum_jobs_cron_tick_lock_is_scoped_to_the_job_queue
    Scenario: The vacuum of another queue is not skipped by the tick of the first
      Given an empty job queue
      And another empty job queue
      And the vacuum cron keeps the jobs of the last 6 hours, in batches of 1000 with at most 200 batches per tick
      And a completed job "old-job" that finished 7 hours ago
      And a completed job "old-job" that finished 7 hours ago, in the other queue
      When the vacuum cron tick is handled
      And the same vacuum cron tick is handled on the other queue
      Then the job "old-job" is purged from the done set, the data and the result
      And the job "old-job" is purged from the other queue too

    @legacy:test_cron::test_refresh_oauth_tokens_cron_tick_lock_is_scoped_to_the_job_queue
    Scenario: The OAuth token refresh of another queue is not skipped by the tick of the first
      Given an empty job queue
      And another empty job queue
      When the refresh OAuth tokens cron tick is handled once on each of the two queues
      Then each of the two queues holds exactly 1 job

  Rule: The vacuum removes the completed jobs past their retention

    Background:
      Given an empty job queue

    @legacy:test_cron::test_vacuum_jobs_cron_tick_purges_only_jobs_completed_before_the_retention_window
    Scenario: Only the jobs completed before the retention are purged
      Given the vacuum cron keeps the jobs of the last 6 hours, in batches of 1000 with at most 200 batches per tick
      And a completed job "old-job" that finished 7 hours ago
      And a completed job "recent-job" that finished 1 hours ago
      And an active job "active-job"
      When the vacuum cron tick is handled
      Then the job "old-job" is purged from the done set, the data and the result
      And the job "recent-job" is kept in the done set, the data and the result
      And the data of the active job "active-job" is kept and the active list still holds 1 job

    @legacy:test_cron::test_vacuum_jobs_cron_tick_purges_every_batch
    Scenario: The vacuum runs batch after batch until nothing is left to purge
      Given the vacuum cron keeps the jobs of the last 6 hours, in batches of 2 with at most 10 batches per tick
      And 5 completed jobs that finished 7 hours ago
      When the vacuum cron tick is handled
      Then 0 completed jobs are left

    @legacy:test_cron::test_vacuum_jobs_cron_tick_stops_at_max_batches_per_tick
    Scenario: The vacuum stops after its maximum number of batches per tick
      Given the vacuum cron keeps the jobs of the last 6 hours, in batches of 2 with at most 1 batches per tick
      And 5 completed jobs that finished 7 hours ago
      When the vacuum cron tick is handled
      Then 3 completed jobs are left
