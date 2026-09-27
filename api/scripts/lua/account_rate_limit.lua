-- Atomically count one request against a per-account fixed-window budget.
--
-- KEYS[1] = budget key (universal-inbox:account-rate-limit:<scope>:<sha256(email)>)
-- ARGV[1] = window_seconds (lifetime of the window opened by the first request)
--
-- Returns: { count, ttl }
--   count - the post-increment number of requests in the current window
--   ttl   - seconds until the window (and the counter) expires
--
-- The TTL is (re)applied whenever the key has none, so the counter can never
-- be left without an expiry, even if a previous EXPIRE was lost.

local count = redis.call('INCR', KEYS[1])
local ttl = redis.call('TTL', KEYS[1])
if ttl < 0 then
    ttl = tonumber(ARGV[1])
    redis.call('EXPIRE', KEYS[1], ttl)
end

return { count, ttl }
