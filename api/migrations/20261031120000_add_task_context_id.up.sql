-- Expand step of the project -> context move: tasks gain a nullable binding to
-- their context while `task.project` (the name) stays authoritative.
--
-- No backfill here: the Todoist project id lives in the encrypted
-- `third_party_item.data_enc`, which SQL cannot read. The sync binds existing
-- tasks by id from application code (`backfill_todoist_task_context_ids`).
ALTER TABLE task
  ADD COLUMN context_id UUID NULL REFERENCES context(id) ON DELETE SET NULL;

CREATE INDEX task_context_idx ON task (context_id);
