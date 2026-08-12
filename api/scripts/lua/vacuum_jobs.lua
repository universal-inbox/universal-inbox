-- Purge a batch of completed apalis jobs from Redis.
--
-- apalis-redis moves a finished job's id into the `done` sorted set and writes
-- its result into the `<data>::result` hash, but never frees the entry in the
-- `data` hash. Its own `vacuum.lua` does not clean `<data>::result` either, and
-- iterates the whole `done` set in a single invocation. This script fixes both:
-- it cleans all 3 keys and only handles `batch_size` ids per call so the Redis
-- event loop is never blocked for long.
--
-- Ids are taken exclusively from the `done` set, so pending, scheduled and
-- in-flight jobs are never touched.
--
-- KEYS[1]: the done jobs sorted set
-- KEYS[2]: the job data hash
-- KEYS[3]: the job data result hash
--
-- ARGV[1]: cutoff timestamp in seconds; only jobs completed before it are purged
-- ARGV[2]: maximum number of jobs to purge in this call
--
-- Returns: the number of purged jobs

local ids = redis.call('ZRANGEBYSCORE', KEYS[1], '-inf', ARGV[1], 'LIMIT', 0, ARGV[2])

for _, id in ipairs(ids) do
    redis.call('HDEL', KEYS[2], id)
    redis.call('HDEL', KEYS[3], id)
    redis.call('ZREM', KEYS[1], id)
end

return #ids
