DROP TRIGGER audit_note_delete ON note;
DROP TRIGGER audit_note_update ON note;
DROP TRIGGER audit_note_insert ON note;
DROP FUNCTION audit_context_note_change();

DROP TRIGGER audit_attachment_delete ON attachment;
DROP TRIGGER audit_attachment_update ON attachment;
DROP TRIGGER audit_attachment_insert ON attachment;
DROP FUNCTION audit_context_attachment_change();
DROP FUNCTION audit_attachment_url(TEXT);

DROP TRIGGER audit_context_delete ON context;
DROP TRIGGER audit_context_update ON context;
DROP TRIGGER audit_context_insert ON context;
DROP FUNCTION audit_context_change();

DELETE FROM audit_event WHERE entity_kind = 'Context';

-- Remove 'Context' from audit_entity_kind (a value cannot be dropped from an enum).
CREATE TYPE audit_entity_kind_new AS ENUM ('Notification', 'Task');

ALTER TABLE audit_event
  ALTER COLUMN entity_kind TYPE audit_entity_kind_new
  USING (entity_kind::text::audit_entity_kind_new);

DROP TYPE audit_entity_kind;
ALTER TYPE audit_entity_kind_new RENAME TO audit_entity_kind;
