-- `kind` was generated from `data->>'type'`: once `data` is encrypted, the application
-- writes it. DROP EXPRESSION keeps the current values.
ALTER TABLE third_party_item ALTER COLUMN kind DROP EXPRESSION;
ALTER TABLE third_party_item ALTER COLUMN kind SET NOT NULL;

-- Encrypted `data` (see api/src/utils/crypto.rs). Ciphertext does not compress: store it
-- uncompressed out of line (EXTERNAL) so reading its key id byte does not detoast it all.
ALTER TABLE third_party_item ADD COLUMN data_enc BYTEA;
ALTER TABLE third_party_item ALTER COLUMN data_enc SET STORAGE EXTERNAL;

-- Plaintext `data` is moved to `data_enc` by `data-encryption encrypt-plaintext` and the
-- column will be dropped by a later migration.
ALTER TABLE third_party_item ALTER COLUMN data DROP NOT NULL;
ALTER TABLE third_party_item
  ADD CONSTRAINT third_party_item_data_check CHECK (data IS NOT NULL OR data_enc IS NOT NULL);
