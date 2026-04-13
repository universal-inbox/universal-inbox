CREATE TABLE slack_bridge_pending_action (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES "user"(id),
    notification_id UUID REFERENCES notification(id),
    action_type TEXT NOT NULL,
    slack_team_id TEXT NOT NULL,
    slack_channel_id TEXT NOT NULL,
    slack_thread_ts TEXT NOT NULL,
    slack_last_message_ts TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Pending',
    failure_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_slack_bridge_pending_action_user_status ON slack_bridge_pending_action (user_id, status);
