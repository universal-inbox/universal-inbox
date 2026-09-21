ALTER TABLE integration_connection
    DROP COLUMN IF EXISTS auto_paused_config_snapshot,
    DROP COLUMN IF EXISTS auto_paused_by_plan_at;
