ALTER TYPE third_party_item_kind ADD VALUE IF NOT EXISTS 'SlackReaction';

UPDATE
  third_party_item
SET
  data = jsonb_set(
    data::jsonb,
    '{content}',
    json_build_object(
      'item', (data->'content'->'starred_item')::jsonb,
      'created_at', (data->'content'->'created_at')::jsonb,
      'state', (data->'content'->'state')::jsonb
    )::jsonb
  )::json
WHERE
  kind = 'SlackStar';

UPDATE
  integration_connection_config
SET
  config = jsonb_set(
    config::jsonb,
    '{content}',
    json_build_object(
      'star_config', (config->>'content')::jsonb,
      'reaction_config', '{"sync_type": {"type": "AsNotifications"}, "reaction_name": "eyes", "sync_enabled": false}'::jsonb
    )::jsonb
  )::json
WHERE
  kind = 'Slack';
