-- ARGV[1]: the prefix pattern of keys to delete
local cursor = "0";
local deleted = 0;
repeat
  local result = redis.call("SCAN", cursor, "MATCH", ARGV[1], "COUNT", 100);
  local keys = result[2];
  for i = 1, #keys do
    local key = keys[i];
    redis.call('del', key);
    deleted = deleted + 1;
  end;
  cursor = result[1];
until cursor == "0";

return deleted;
