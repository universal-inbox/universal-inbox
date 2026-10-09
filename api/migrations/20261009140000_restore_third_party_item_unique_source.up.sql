-- `20260413075607_remove_slack_star_support` dropped and re-added `kind`, which also dropped
-- UNIQUE(source_id, kind, integration_connection_id) and its index. Without them, the sync
-- lookup scans the whole table and a concurrent sync can insert a duplicate item.

-- Delete duplicates first, keeping the item a task or a notification uses, then the most
-- recently updated one. Deleting an item cascades to its notification and task.
DO $$
DECLARE
  deleted_count INTEGER;
BEGIN
  WITH ranked AS (
    SELECT
      item.id,
      row_number() OVER (
        PARTITION BY item.source_id, item.kind, item.integration_connection_id
        ORDER BY
          (
            EXISTS (SELECT 1 FROM task WHERE task.source_item_id = item.id OR task.sink_item_id = item.id)
            OR EXISTS (SELECT 1 FROM notification WHERE notification.source_item_id = item.id)
          ) DESC,
          item.updated_at DESC,
          item.id
      ) AS rank
    FROM third_party_item AS item
  )
  DELETE FROM third_party_item
  WHERE id IN (SELECT id FROM ranked WHERE rank > 1);
  GET DIAGNOSTICS deleted_count = ROW_COUNT;
  RAISE NOTICE 'Deleted % duplicate third_party_item rows', deleted_count;
END $$;

-- Its index also serves the lookups by `source_id` and `(source_id, kind)`: the former
-- `third_party_item_source_id_kind_idx` is not recreated.
ALTER TABLE third_party_item
  ADD CONSTRAINT third_party_item_source_id_kind_integration_connection_id_key
  UNIQUE (source_id, kind, integration_connection_id);
