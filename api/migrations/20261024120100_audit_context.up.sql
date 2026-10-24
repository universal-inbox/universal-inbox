-- Audit triggers of the `context`, `attachment` and `note` tables (see
-- 20261012100000_create_audit_event.up.sql). They ship with the tables: `app.audit_strict` only
-- rejects the writes of tables that already have triggers.
--
-- A new enum value cannot be used in the transaction that adds it, which is why this is not part
-- of the migration creating the tables.

ALTER TYPE audit_entity_kind ADD VALUE IF NOT EXISTS 'Context';

CREATE FUNCTION audit_context_change() RETURNS TRIGGER AS $$
DECLARE
  changes JSONB := '{}'::JSONB;
  actor JSONB;
BEGIN
  -- Deleting the user cascades to their contexts: their audit rows go with them, so nothing is
  -- recorded for a user that no longer exists.
  IF TG_OP <> 'INSERT' AND NOT EXISTS (SELECT 1 FROM "user" WHERE id = OLD.user_id) THEN
    RETURN NULL;
  END IF;

  actor := audit_actor(TG_OP, TG_TABLE_NAME);

  IF TG_OP = 'DELETE' THEN
    INSERT INTO audit_event
      (user_id, entity_kind, entity_id, action, title, source_kind, changes, actor, actor_kind)
    VALUES
      (OLD.user_id, 'Context', OLD.id, 'Removed', OLD.name, COALESCE(OLD.binding_provider::TEXT, 'Local'),
       jsonb_build_object('status', audit_change(OLD.status, NULL)), actor, actor->>'kind');
    RETURN NULL;
  END IF;

  IF TG_OP = 'INSERT' THEN
    changes := jsonb_build_object('status', audit_change(NULL, NEW.status));
    INSERT INTO audit_event
      (user_id, entity_kind, entity_id, action, title, source_kind, changes, params, actor, actor_kind)
    VALUES
      (NEW.user_id, 'Context', NEW.id, 'Created', NEW.name, COALESCE(NEW.binding_provider::TEXT, 'Local'),
       changes,
       jsonb_build_object(
         'name', NEW.name,
         'parent_id', NEW.parent_id,
         'binding_provider', NEW.binding_provider,
         'binding_source_id', NEW.binding_source_id,
         'binding_mirrored', NEW.binding_mirrored
       ),
       actor, actor->>'kind');
    RETURN NULL;
  END IF;

  IF OLD.name IS DISTINCT FROM NEW.name THEN
    changes := changes || jsonb_build_object('name', audit_change(OLD.name, NEW.name));
  END IF;
  IF OLD.parent_id IS DISTINCT FROM NEW.parent_id THEN
    changes := changes || jsonb_build_object('parent_id', audit_change(OLD.parent_id, NEW.parent_id));
  END IF;
  IF OLD.status IS DISTINCT FROM NEW.status THEN
    changes := changes || jsonb_build_object('status', audit_change(OLD.status, NEW.status));
  END IF;
  IF OLD.binding_provider IS DISTINCT FROM NEW.binding_provider THEN
    changes := changes || jsonb_build_object('binding_provider', audit_change(OLD.binding_provider, NEW.binding_provider));
  END IF;
  IF OLD.binding_source_id IS DISTINCT FROM NEW.binding_source_id THEN
    changes := changes || jsonb_build_object('binding_source_id', audit_change(OLD.binding_source_id, NEW.binding_source_id));
  END IF;
  IF OLD.binding_mirrored IS DISTINCT FROM NEW.binding_mirrored THEN
    changes := changes || jsonb_build_object('binding_mirrored', audit_change(OLD.binding_mirrored, NEW.binding_mirrored));
  END IF;

  INSERT INTO audit_event
    (user_id, entity_kind, entity_id, action, title, source_kind, changes, actor, actor_kind)
  VALUES
    (NEW.user_id, 'Context', NEW.id, 'Updated', NEW.name, COALESCE(NEW.binding_provider::TEXT, 'Local'),
     changes, actor, actor->>'kind');
  RETURN NULL;
END
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_context_insert AFTER INSERT ON context
  FOR EACH ROW EXECUTE FUNCTION audit_context_change();

CREATE TRIGGER audit_context_update AFTER UPDATE ON context
  FOR EACH ROW
    WHEN (
      OLD.name IS DISTINCT FROM NEW.name
      OR OLD.parent_id IS DISTINCT FROM NEW.parent_id
      OR OLD.status IS DISTINCT FROM NEW.status
      OR OLD.binding_provider IS DISTINCT FROM NEW.binding_provider
      OR OLD.binding_source_id IS DISTINCT FROM NEW.binding_source_id
      OR OLD.binding_mirrored IS DISTINCT FROM NEW.binding_mirrored
    )
    EXECUTE FUNCTION audit_context_change();

CREATE TRIGGER audit_context_delete AFTER DELETE ON context
  FOR EACH ROW EXECUTE FUNCTION audit_context_change();

-- An attachment is recorded against its owner (the context or the task it is attached to):
-- `entity_kind` is the owner's kind, `entity_id` the attachment, `related_entity_id` the owner,
-- so that the history of a context (filtering on `entity_id`) includes its attachments.
--
-- The URL is copied without its query string and fragment: they can carry signed tokens, and the
-- audit row outlives the attachment.
CREATE FUNCTION audit_attachment_url(url TEXT) RETURNS TEXT AS $$
  SELECT regexp_replace(url, '[?#].*$', '');
$$ LANGUAGE sql IMMUTABLE;

CREATE FUNCTION audit_context_attachment_change() RETURNS TRIGGER AS $$
DECLARE
  changes JSONB := '{}'::JSONB;
  actor JSONB;
  attachment_row attachment;
BEGIN
  IF TG_OP <> 'INSERT' AND NOT EXISTS (SELECT 1 FROM "user" WHERE id = OLD.user_id) THEN
    RETURN NULL;
  END IF;

  actor := audit_actor(TG_OP, TG_TABLE_NAME);

  IF TG_OP = 'DELETE' THEN
    attachment_row := OLD;
    changes := jsonb_build_object('url', audit_change(audit_attachment_url(OLD.url), NULL));
  ELSIF TG_OP = 'INSERT' THEN
    attachment_row := NEW;
    changes := jsonb_build_object('url', audit_change(NULL, audit_attachment_url(NEW.url)));
  ELSE
    attachment_row := NEW;
    IF OLD.url IS DISTINCT FROM NEW.url THEN
      changes := changes || jsonb_build_object('url', audit_change(audit_attachment_url(OLD.url), audit_attachment_url(NEW.url)));
    END IF;
    IF OLD.title IS DISTINCT FROM NEW.title THEN
      changes := changes || jsonb_build_object('title', audit_change(OLD.title, NEW.title));
    END IF;
    IF OLD.position IS DISTINCT FROM NEW.position THEN
      changes := changes || jsonb_build_object('position', audit_change(OLD.position, NEW.position));
    END IF;
  END IF;

  INSERT INTO audit_event
    (user_id, entity_kind, entity_id, related_entity_id, action, title, source_kind, changes, params, actor, actor_kind)
  VALUES
    (attachment_row.user_id,
     CASE WHEN attachment_row.context_id IS NOT NULL THEN 'Context' ELSE 'Task' END::audit_entity_kind,
     attachment_row.id,
     COALESCE(attachment_row.context_id, attachment_row.task_id),
     CASE TG_OP WHEN 'INSERT' THEN 'Created' WHEN 'DELETE' THEN 'Removed' ELSE 'Updated' END,
     COALESCE(attachment_row.title, audit_attachment_url(attachment_row.url)),
     attachment_row.provider::TEXT,
     changes,
     jsonb_build_object('resource', 'Attachment', 'kind', attachment_row.kind),
     actor, actor->>'kind');
  RETURN NULL;
END
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_attachment_insert AFTER INSERT ON attachment
  FOR EACH ROW EXECUTE FUNCTION audit_context_attachment_change();

CREATE TRIGGER audit_attachment_update AFTER UPDATE ON attachment
  FOR EACH ROW
    WHEN (
      OLD.url IS DISTINCT FROM NEW.url
      OR OLD.title IS DISTINCT FROM NEW.title
      OR OLD.position IS DISTINCT FROM NEW.position
    )
    EXECUTE FUNCTION audit_context_attachment_change();

CREATE TRIGGER audit_attachment_delete AFTER DELETE ON attachment
  FOR EACH ROW EXECUTE FUNCTION audit_context_attachment_change();

-- The note of a context is recorded against the context. The body is never read: only whether
-- there is one.
CREATE FUNCTION audit_context_note_change() RETURNS TRIGGER AS $$
DECLARE
  note_row note;
  owner context;
  actor JSONB;
BEGIN
  IF TG_OP = 'DELETE' THEN
    note_row := OLD;
  ELSE
    note_row := NEW;
  END IF;

  -- The note is deleted with its context (the context is audited), or with its user.
  SELECT * INTO owner FROM context WHERE id = note_row.context_id;
  IF NOT FOUND OR NOT EXISTS (SELECT 1 FROM "user" WHERE id = owner.user_id) THEN
    RETURN NULL;
  END IF;

  actor := audit_actor(TG_OP, TG_TABLE_NAME);

  INSERT INTO audit_event
    (user_id, entity_kind, entity_id, related_entity_id, action, title, source_kind, changes, params, actor, actor_kind)
  VALUES
    (owner.user_id, 'Context', note_row.id, note_row.context_id,
     CASE TG_OP WHEN 'INSERT' THEN 'Created' WHEN 'DELETE' THEN 'Removed' ELSE 'Updated' END,
     owner.name, COALESCE(owner.binding_provider::TEXT, 'Local'),
     '{}'::JSONB,
     jsonb_build_object(
       'resource', 'Note',
       -- The body is encrypted and never copied, only whether one is set. An encrypted empty
       -- body is 31 bytes: 3 header + 12 nonce + 16 tag (api/src/utils/crypto.rs).
       'body_present', octet_length(note_row.body_enc) > 31
     ),
     actor, actor->>'kind');
  RETURN NULL;
END
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_note_insert AFTER INSERT ON note
  FOR EACH ROW EXECUTE FUNCTION audit_context_note_change();

CREATE TRIGGER audit_note_update AFTER UPDATE ON note
  FOR EACH ROW
    WHEN (OLD.body_enc IS DISTINCT FROM NEW.body_enc)
    EXECUTE FUNCTION audit_context_note_change();

CREATE TRIGGER audit_note_delete AFTER DELETE ON note
  FOR EACH ROW EXECUTE FUNCTION audit_context_note_change();
