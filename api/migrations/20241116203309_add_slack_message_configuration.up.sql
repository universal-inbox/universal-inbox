ALTER TYPE third_party_item_kind ADD VALUE IF NOT EXISTS 'SlackThread';

UPDATE
    integration_connection_config
SET
    config = jsonb_set(
        config::jsonb,
        '{content}',
        json_build_object(
            'star_config', (config -> 'content' -> 'star_config')::jsonb,
            'reaction_config',
            (config -> 'content' -> 'reaction_config')::jsonb,
            'message_config', '{"sync_enabled": false, "is_2way_sync": false}'::jsonb
        )::jsonb
    )::json
WHERE
    kind = 'Slack';
