-- Deleting a user (account deletion) or a notification that still has
-- slack_bridge_pending_action rows failed with a foreign-key violation: both
-- constraints were created without an ON DELETE clause.
--
-- user_id: a pending action belongs to its user, delete it with the user.
-- notification_id: the browser extension replays a pending action from its
-- Slack coordinates (team / channel / thread), the notification id is only a
-- back-reference. Keep the action (so the user's Slack-side action still
-- happens) and just drop the reference.
ALTER TABLE slack_bridge_pending_action
    DROP CONSTRAINT slack_bridge_pending_action_user_id_fkey,
    ADD CONSTRAINT slack_bridge_pending_action_user_id_fkey
        FOREIGN KEY (user_id) REFERENCES "user"(id) ON DELETE CASCADE;

ALTER TABLE slack_bridge_pending_action
    DROP CONSTRAINT slack_bridge_pending_action_notification_id_fkey,
    ADD CONSTRAINT slack_bridge_pending_action_notification_id_fkey
        FOREIGN KEY (notification_id) REFERENCES notification(id) ON DELETE SET NULL;
