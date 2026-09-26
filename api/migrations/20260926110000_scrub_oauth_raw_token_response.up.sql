-- The token-refresh path stored provider token responses without
-- sanitizing them; for Slack the whole token body (cleartext access and
-- refresh tokens) ended up in `raw_token_response`, next to the encrypted
-- copies. Remove every `access_token` / `refresh_token` / `id_token` key, at
-- any depth, from the rows already stored. The encrypted columns are the
-- source of truth and are untouched.
CREATE FUNCTION pg_temp.strip_oauth_secrets(value jsonb) RETURNS jsonb
LANGUAGE plpgsql IMMUTABLE AS $$
BEGIN
  IF jsonb_typeof(value) = 'object' THEN
    RETURN COALESCE(
      (SELECT jsonb_object_agg(key, pg_temp.strip_oauth_secrets(val))
       FROM jsonb_each(value) AS e(key, val)
       WHERE key NOT IN ('access_token', 'refresh_token', 'id_token')),
      '{}'::jsonb
    );
  ELSIF jsonb_typeof(value) = 'array' THEN
    RETURN COALESCE(
      (SELECT jsonb_agg(pg_temp.strip_oauth_secrets(elem) ORDER BY ord)
       FROM jsonb_array_elements(value) WITH ORDINALITY AS a(elem, ord)),
      '[]'::jsonb
    );
  END IF;
  RETURN value;
END
$$;

UPDATE oauth_credential
SET raw_token_response = pg_temp.strip_oauth_secrets(raw_token_response)
WHERE raw_token_response IS DISTINCT FROM pg_temp.strip_oauth_secrets(raw_token_response);

DROP FUNCTION pg_temp.strip_oauth_secrets(jsonb);
