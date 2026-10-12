-- Audit log of notification and task changes (see src/audit.rs).
--
-- Rows are written by the triggers below, so no write path can skip them. The actor comes from
-- the transaction-local `app.audit_actor` setting, set by `Repository::begin` from the request,
-- MCP call, job or CLI command that opened the transaction.

CREATE TYPE audit_entity_kind AS ENUM ('Notification', 'Task');

CREATE TABLE audit_event (
  id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id           UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  entity_kind       audit_entity_kind NOT NULL,
  -- No foreign key: the history outlives the notification or task.
  entity_id         UUID NOT NULL,
  -- For a notification event, the task it is linked to.
  related_entity_id UUID,
  action            TEXT NOT NULL,
  -- Already plaintext in `notification.title` / `task.title`. Bodies are never copied.
  title             TEXT NOT NULL,
  source_kind       TEXT,
  changes           JSONB NOT NULL,
  params            JSONB,
  actor             JSONB NOT NULL,
  actor_kind        TEXT NOT NULL,
  -- Groups the events of one action (eg. a bulk update).
  transaction_id    BIGINT NOT NULL DEFAULT txid_current(),
  created_at        TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX audit_event_user_created_idx ON audit_event (user_id, created_at DESC, id DESC);
CREATE INDEX audit_event_user_entity_idx ON audit_event (user_id, entity_id, created_at DESC);
CREATE INDEX audit_event_user_related_idx ON audit_event (user_id, related_entity_id)
  WHERE related_entity_id IS NOT NULL;
CREATE INDEX audit_event_created_idx ON audit_event (created_at);

-- Actor of the current transaction. Without one, the change is recorded as `Unknown`, or
-- rejected when `app.audit_strict` is on (tests), so that a write path missing its actor fails.
CREATE FUNCTION audit_actor(operation TEXT, table_name TEXT) RETURNS JSONB AS $$
DECLARE
  actor TEXT := current_setting('app.audit_actor', true);
BEGIN
  IF actor IS NULL OR actor = '' THEN
    IF current_setting('app.audit_strict', true) = 'on' THEN
      RAISE EXCEPTION 'audit: % on % without an audit actor in context', operation, table_name;
    END IF;
    RETURN '{"kind": "Unknown"}'::JSONB;
  END IF;
  RETURN actor::JSONB;
END
$$ LANGUAGE plpgsql STABLE;

CREATE FUNCTION audit_change(old_value ANYELEMENT, new_value ANYELEMENT) RETURNS JSONB AS $$
  SELECT jsonb_build_object('from', to_jsonb(old_value), 'to', to_jsonb(new_value));
$$ LANGUAGE sql IMMUTABLE;

CREATE FUNCTION audit_notification_change() RETURNS TRIGGER AS $$
DECLARE
  changes JSONB := '{}'::JSONB;
  actor JSONB;
BEGIN
  -- Deleting the user cascades to their notifications (deleted, or `task_id` set to NULL): their
  -- audit rows go with them, so nothing is recorded for a user that no longer exists.
  IF TG_OP <> 'INSERT' AND NOT EXISTS (SELECT 1 FROM "user" WHERE id = OLD.user_id) THEN
    RETURN NULL;
  END IF;

  IF TG_OP = 'DELETE' THEN
    actor := audit_actor(TG_OP, TG_TABLE_NAME);
    INSERT INTO audit_event
      (user_id, entity_kind, entity_id, related_entity_id, action, title, source_kind, changes, actor, actor_kind)
    VALUES
      (OLD.user_id, 'Notification', OLD.id, OLD.task_id, 'Removed', OLD.title, OLD.kind::TEXT,
       jsonb_build_object('status', audit_change(OLD.status, NULL)), actor, actor->>'kind');
    RETURN NULL;
  END IF;

  IF TG_OP = 'INSERT' THEN
    changes := jsonb_build_object('status', audit_change(NULL, NEW.status));
    IF NEW.snoozed_until IS NOT NULL THEN
      changes := changes || jsonb_build_object('snoozed_until', audit_change(NULL, NEW.snoozed_until));
    END IF;
    IF NEW.task_id IS NOT NULL THEN
      changes := changes || jsonb_build_object('task_id', audit_change(NULL, NEW.task_id));
    END IF;
  ELSE
    IF OLD.status IS DISTINCT FROM NEW.status THEN
      changes := changes || jsonb_build_object('status', audit_change(OLD.status, NEW.status));
    END IF;
    IF OLD.snoozed_until IS DISTINCT FROM NEW.snoozed_until THEN
      changes := changes || jsonb_build_object('snoozed_until', audit_change(OLD.snoozed_until, NEW.snoozed_until));
    END IF;
    IF OLD.set_aside_at IS DISTINCT FROM NEW.set_aside_at THEN
      changes := changes || jsonb_build_object('set_aside_at', audit_change(OLD.set_aside_at, NEW.set_aside_at));
    END IF;
    IF OLD.task_id IS DISTINCT FROM NEW.task_id THEN
      changes := changes || jsonb_build_object('task_id', audit_change(OLD.task_id, NEW.task_id));
    END IF;
  END IF;

  actor := audit_actor(TG_OP, TG_TABLE_NAME);
  INSERT INTO audit_event
    (user_id, entity_kind, entity_id, related_entity_id, action, title, source_kind, changes, actor, actor_kind)
  VALUES
    (NEW.user_id, 'Notification', NEW.id, NEW.task_id,
     CASE TG_OP WHEN 'INSERT' THEN 'Created' ELSE 'Updated' END,
     NEW.title, NEW.kind::TEXT, changes, actor, actor->>'kind');
  RETURN NULL;
END
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_notification_insert AFTER INSERT ON notification
  FOR EACH ROW EXECUTE FUNCTION audit_notification_change();

CREATE TRIGGER audit_notification_update AFTER UPDATE ON notification
  FOR EACH ROW
    WHEN (
      OLD.status IS DISTINCT FROM NEW.status
      OR OLD.snoozed_until IS DISTINCT FROM NEW.snoozed_until
      OR OLD.set_aside_at IS DISTINCT FROM NEW.set_aside_at
      OR OLD.task_id IS DISTINCT FROM NEW.task_id
    )
    EXECUTE FUNCTION audit_notification_change();

CREATE TRIGGER audit_notification_delete AFTER DELETE ON notification
  FOR EACH ROW EXECUTE FUNCTION audit_notification_change();

CREATE FUNCTION audit_task_change() RETURNS TRIGGER AS $$
DECLARE
  actor JSONB;
  task_row JSONB;
BEGIN
  -- Deleting the user cascades to their tasks (deleted, or `task_id` set to NULL): their
  -- audit rows go with them, so nothing is recorded for a user that no longer exists.
  IF TG_OP <> 'INSERT' AND NOT EXISTS (SELECT 1 FROM "user" WHERE id = OLD.user_id) THEN
    RETURN NULL;
  END IF;

  IF TG_OP = 'DELETE' THEN
    actor := audit_actor(TG_OP, TG_TABLE_NAME);
    INSERT INTO audit_event
      (user_id, entity_kind, entity_id, action, title, source_kind, changes, actor, actor_kind)
    VALUES
      (OLD.user_id, 'Task', OLD.id, 'Removed', OLD.title, OLD.kind::TEXT,
       jsonb_build_object('status', audit_change(OLD.status, NULL)), actor, actor->>'kind');
    RETURN NULL;
  END IF;

  actor := audit_actor(TG_OP, TG_TABLE_NAME);

  IF TG_OP = 'INSERT' THEN
    -- Read through JSON so that dropping the plaintext `body` column does not break the trigger.
    task_row := to_jsonb(NEW);
    INSERT INTO audit_event
      (user_id, entity_kind, entity_id, action, title, source_kind, changes, params, actor, actor_kind)
    VALUES
      (NEW.user_id, 'Task', NEW.id, 'Created', NEW.title, NEW.kind::TEXT,
       jsonb_build_object('status', audit_change(NULL, NEW.status)),
       jsonb_build_object(
         'kind', NEW.kind,
         'project', NEW.project,
         'title', NEW.title,
         'due_at', NEW.due_at,
         'priority', NEW.priority,
         -- The body is encrypted and never copied, only whether one is set. An encrypted empty
         -- body is 31 bytes: 3 header + 12 nonce + 16 tag (api/src/utils/crypto.rs).
         'body_present', COALESCE(octet_length(NEW.body_enc) > 31, false)
                         OR COALESCE(task_row->>'body', '') <> ''
       ),
       actor, actor->>'kind');
  ELSE
    INSERT INTO audit_event
      (user_id, entity_kind, entity_id, action, title, source_kind, changes, actor, actor_kind)
    VALUES
      (NEW.user_id, 'Task', NEW.id, 'Updated', NEW.title, NEW.kind::TEXT,
       jsonb_build_object('status', audit_change(OLD.status, NEW.status)), actor, actor->>'kind');
  END IF;
  RETURN NULL;
END
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_task_insert AFTER INSERT ON task
  FOR EACH ROW EXECUTE FUNCTION audit_task_change();

CREATE TRIGGER audit_task_update AFTER UPDATE ON task
  FOR EACH ROW
    WHEN (OLD.status IS DISTINCT FROM NEW.status)
    EXECUTE FUNCTION audit_task_change();

CREATE TRIGGER audit_task_delete AFTER DELETE ON task
  FOR EACH ROW EXECUTE FUNCTION audit_task_change();
