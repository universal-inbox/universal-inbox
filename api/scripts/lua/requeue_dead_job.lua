-- Put a dead apalis job back on the queue.
--
-- apalis-redis has no API to requeue a dead job: this script does what its
-- `push_job.lua` does for a new job, but only for a job still in the `dead`
-- set, so a job requeued by a concurrent run is not pushed twice. The job data
-- is replaced by the given payload (its attempts reset by the caller).
--
-- KEYS[1]: the dead jobs sorted set
-- KEYS[2]: the job data hash
-- KEYS[3]: the active jobs list
-- KEYS[4]: the signal list
--
-- ARGV[1]: the job id
-- ARGV[2]: the new job payload
--
-- Returns: 1 if the job was requeued, 0 if it was not in the dead set

if redis.call('ZREM', KEYS[1], ARGV[1]) == 0 then
    return 0
end

redis.call('HSET', KEYS[2], ARGV[1], ARGV[2])
redis.call('RPUSH', KEYS[3], ARGV[1])

-- Signal that there are jobs in the queue
redis.call('DEL', KEYS[4])
redis.call('LPUSH', KEYS[4], 1)

return 1
