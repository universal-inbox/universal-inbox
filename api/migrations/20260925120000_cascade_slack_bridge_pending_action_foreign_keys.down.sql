ALTER TABLE slack_bridge_pending_action
    DROP CONSTRAINT slack_bridge_pending_action_user_id_fkey,
    ADD CONSTRAINT slack_bridge_pending_action_user_id_fkey
        FOREIGN KEY (user_id) REFERENCES "user"(id);

ALTER TABLE slack_bridge_pending_action
    DROP CONSTRAINT slack_bridge_pending_action_notification_id_fkey,
    ADD CONSTRAINT slack_bridge_pending_action_notification_id_fkey
        FOREIGN KEY (notification_id) REFERENCES notification(id);
