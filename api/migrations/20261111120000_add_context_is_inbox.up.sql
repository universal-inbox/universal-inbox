-- The inbox of a task manager, found by the provider's own flag (Todoist `inbox_project`), never
-- by the name "Inbox": a user project named "Inbox" is not the inbox. Set by the context mirror.
ALTER TABLE context
  ADD COLUMN is_inbox BOOLEAN NOT NULL DEFAULT false;

-- At most one inbox per user and provider
CREATE UNIQUE INDEX context_one_inbox_per_provider_idx
  ON context (user_id, binding_provider)
  WHERE is_inbox;
