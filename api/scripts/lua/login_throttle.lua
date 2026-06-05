-- Atomically record a failed login attempt for one account and decide whether
-- the account is now (or remains) locked, using exponential backoff.
--
-- KEYS[1] = throttle key (universal-inbox:login-throttle:<sha256(email)>)
-- ARGV[1] = now              (epoch seconds)
-- ARGV[2] = max_attempts     (failures tolerated before locking)
-- ARGV[3] = window_seconds   (sliding window the counter decays over)
-- ARGV[4] = base_seconds     (lockout applied the first time the threshold is hit)
-- ARGV[5] = max_seconds      (cap on the exponential lockout duration)
--
-- Returns: { fail_count, locked_until, newly_locked }
--   fail_count   - the post-increment consecutive-failure count
--   locked_until - epoch seconds the lock expires (0 if not locked)
--   newly_locked - 1 if THIS failure started a fresh lock episode, else 0
--                  (used by the caller to send the lockout email exactly once)

local now          = tonumber(ARGV[1])
local max_attempts = tonumber(ARGV[2])
local window       = tonumber(ARGV[3])
local base         = tonumber(ARGV[4])
local max_lock     = tonumber(ARGV[5])

local count = redis.call('HINCRBY', KEYS[1], 'fail_count', 1)
local locked_until = 0
local newly_locked = 0

if count >= max_attempts then
    -- base * 2^(count - max_attempts): base at the threshold, doubling per
    -- extra failure, capped at max_lock.
    local exponent = count - max_attempts
    local lock = base * (2 ^ exponent)
    if lock > max_lock then
        lock = max_lock
    end
    lock = math.floor(lock)
    locked_until = now + lock

    -- A fresh lock episode is one where the account was not already locked.
    -- The caller only reaches this script after the read-only check passed,
    -- so this is normally true, but we compute it defensively.
    local prev = tonumber(redis.call('HGET', KEYS[1], 'locked_until')) or 0
    if prev <= now then
        newly_locked = 1
    end
    redis.call('HSET', KEYS[1], 'locked_until', locked_until)
end

-- Expire the key after the longer of the attempt window and the active
-- lockout, so state decays automatically once the attacker gives up.
local ttl = window
local remaining = locked_until - now
if remaining > ttl then
    ttl = remaining
end
redis.call('EXPIRE', KEYS[1], ttl)

return { count, locked_until, newly_locked }
