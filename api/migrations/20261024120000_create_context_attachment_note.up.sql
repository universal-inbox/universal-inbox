-- Contexts: a tree of named contexts, each with exactly one note, plus attachments that point
-- at a context or a task. See src/context/mod.rs.

CREATE TYPE context_status AS ENUM ('Active', 'Archived', 'Tombstoned');
CREATE TYPE attachment_kind AS ENUM ('Url', 'ThirdPartyItem');

CREATE TABLE context (
  id                UUID PRIMARY KEY,
  user_id           UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  -- Removing a parent never removes its children: they become roots.
  parent_id         UUID REFERENCES context(id) ON DELETE SET NULL,
  name              TEXT NOT NULL,
  -- The upstream project the context is bound to. All NULL = UI-owned context, with no upstream.
  -- Bound on the provider's project id, never on its display name.
  binding_provider  integration_provider_kind,
  binding_source_id TEXT,
  -- true: upstream owns `name` and `parent_id`
  binding_mirrored  BOOLEAN,
  status            context_status NOT NULL DEFAULT 'Active',
  -- Exactly one note per context. The note row is inserted in the same transaction (see the
  -- deferred foreign key below), so a context without a note cannot be committed.
  note_id           UUID NOT NULL,
  created_at        TIMESTAMP NOT NULL,
  updated_at        TIMESTAMP NOT NULL,

  -- Only the one-row cycle is excluded here: longer cycles are the business of the tree writers
  -- (tree sync), which know the tree.
  CONSTRAINT context_parent_not_self CHECK (parent_id IS NULL OR parent_id <> id),
  CONSTRAINT context_binding_all_or_none CHECK (
    (binding_provider IS NULL AND binding_source_id IS NULL AND binding_mirrored IS NULL)
    OR (binding_provider IS NOT NULL AND binding_source_id IS NOT NULL AND binding_mirrored IS NOT NULL)
  ),
  CONSTRAINT context_binding_provider_is_task_service CHECK (
    binding_provider IS NULL OR binding_provider IN ('Todoist', 'TickTick')
  ),
  -- Rows with an unbound context have NULL columns, which a unique constraint does not compare.
  CONSTRAINT context_binding_unique UNIQUE (user_id, binding_provider, binding_source_id)
);

CREATE INDEX context_user_parent_idx ON context (user_id, parent_id);

CREATE TABLE note (
  id              UUID PRIMARY KEY,
  context_id      UUID NOT NULL UNIQUE REFERENCES context(id) ON DELETE CASCADE,
  -- Markdown, owned by Universal Inbox, encrypted like `task.body_enc` (see
  -- api/src/repository/data_encryption.rs). Never copied into the audit log.
  body_enc        BYTEA NOT NULL,
  -- 'Local' in Phase 1. Text and JSONB, not an enum, so that an external backend needs no migration.
  backend_kind    TEXT NOT NULL DEFAULT 'Local',
  backend_binding JSONB,
  created_at      TIMESTAMP NOT NULL,
  updated_at      TIMESTAMP NOT NULL,

  CONSTRAINT note_external_backend_has_binding CHECK (
    backend_kind = 'Local' OR backend_binding IS NOT NULL
  )
);

ALTER TABLE context
  ADD CONSTRAINT context_note_id_fkey
  FOREIGN KEY (note_id) REFERENCES note(id) DEFERRABLE INITIALLY DEFERRED;

CREATE TABLE attachment (
  id         UUID PRIMARY KEY,
  user_id    UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  -- Attached to exactly one of a context or a task. Two columns rather than a polymorphic id,
  -- for referential integrity and cascade.
  context_id UUID REFERENCES context(id) ON DELETE CASCADE,
  task_id    UUID REFERENCES task(id) ON DELETE CASCADE,
  kind       attachment_kind NOT NULL,
  url        TEXT NOT NULL,
  title      TEXT,
  provider   integration_provider_kind,
  position   INTEGER NOT NULL DEFAULT 0,
  created_at TIMESTAMP NOT NULL,
  updated_at TIMESTAMP NOT NULL,

  CONSTRAINT attachment_has_one_target CHECK (num_nonnulls(context_id, task_id) = 1)
);

CREATE INDEX attachment_context_position_idx ON attachment (context_id, position) WHERE context_id IS NOT NULL;
CREATE INDEX attachment_task_position_idx ON attachment (task_id, position) WHERE task_id IS NOT NULL;
CREATE INDEX attachment_user_idx ON attachment (user_id);
