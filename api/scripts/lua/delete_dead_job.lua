-- Delete a dead apalis job from Redis.
--
-- Cleans the 3 keys holding a dead job: its id in the `dead` set, its payload
-- in the `data` hash and its last error in the `<data>::result` hash. Only a
-- job still in the `dead` set is deleted, so a job requeued meanwhile is kept.
-- One job per call so the Redis event loop is never blocked on a large set.
--
-- KEYS[1]: the dead jobs sorted set
-- KEYS[2]: the job data hash
-- KEYS[3]: the job data result hash
--
-- ARGV[1]: the job id
--
-- Returns: 1 if the job was deleted, 0 if it was not in the dead set

if redis.call('ZREM', KEYS[1], ARGV[1]) == 0 then
    return 0
end

redis.call('HDEL', KEYS[2], ARGV[1])
redis.call('HDEL', KEYS[3], ARGV[1])

return 1
