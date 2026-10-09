-- Only possible before `data-encryption encrypt-plaintext` moved data to `data_enc`:
-- encrypted values cannot be decrypted in SQL.
ALTER TABLE third_party_item DROP CONSTRAINT third_party_item_data_check;
ALTER TABLE third_party_item ALTER COLUMN data SET NOT NULL;
ALTER TABLE third_party_item DROP COLUMN data_enc;

ALTER TABLE third_party_item DROP COLUMN kind;
ALTER TABLE third_party_item
  ADD COLUMN kind THIRD_PARTY_ITEM_KIND GENERATED ALWAYS AS (text_to_third_party_item_kind(data ->> 'type')) STORED;
