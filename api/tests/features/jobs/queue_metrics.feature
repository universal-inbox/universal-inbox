Feature: Sampling the job queue
  A sample of the job queue counts the pending, running, failed and dead jobs
  and tells how long the oldest pending job has been waiting.

  @legacy:test_job_metrics::test_sample_of_an_empty_queue
  Scenario: An empty queue has nothing in it
    Given an empty job queue
    When I sample the queue
    Then nothing is pending, running, failed or dead, and no pending job has an age

  @legacy:test_job_metrics::test_sample_counts_pending_jobs_and_ages_the_oldest
  Scenario: Pending jobs are counted and the oldest one gives the age
    Given pending jobs that were enqueued 60 seconds ago and 10 seconds ago
    When I sample the queue
    Then the sample shows 2 pending and 0 running jobs
    And the oldest pending job is 60 seconds old

  @legacy:test_job_metrics::test_sample_age_is_zero_when_the_oldest_job_has_no_enqueue_time
  Scenario: A pending job without an enqueue time has no age
    Given a pending job that has no enqueue time
    When I sample the queue
    Then the sample shows 1 pending and 0 running job
    And the oldest pending job is 0 seconds old
