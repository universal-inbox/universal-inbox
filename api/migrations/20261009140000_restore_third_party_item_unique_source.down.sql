-- Duplicates deleted by the up migration are not restored.
ALTER TABLE third_party_item
  DROP CONSTRAINT third_party_item_source_id_kind_integration_connection_id_key;
