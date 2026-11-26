Feature: Sampling the job queue
  A sample of the job queue counts the pending, running, failed and dead jobs
  and tells how long the oldest pending job has been waiting.

  Scenario: An empty queue has nothing in it
    Given an empty job queue
    When I sample the queue
    Then nothing is pending, running, failed or dead, and no pending job has an age

  Scenario: Pending jobs are counted and the oldest one gives the age
    Given pending jobs that were enqueued 60 seconds ago and 10 seconds ago
    When I sample the queue
    Then the sample shows 2 pending and 0 running jobs
    And the oldest pending job is 60 seconds old

  Scenario: A pending job without an enqueue time has no age
    Given a pending job that has no enqueue time
    When I sample the queue
    Then the sample shows 1 pending and 0 running job
    And the oldest pending job is 0 seconds old
