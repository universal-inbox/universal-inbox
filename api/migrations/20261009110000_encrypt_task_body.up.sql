-- Encrypted `body` (see api/src/utils/crypto.rs), stored out of line uncompressed
-- (ciphertext does not compress).
ALTER TABLE task ADD COLUMN body_enc BYTEA;
ALTER TABLE task ALTER COLUMN body_enc SET STORAGE EXTERNAL;

-- Plaintext `body` is moved to `body_enc` by `data-encryption encrypt-plaintext` and the
-- column will be dropped by a later migration.
ALTER TABLE task ALTER COLUMN body DROP NOT NULL;
ALTER TABLE task ADD CONSTRAINT task_body_check CHECK (body IS NOT NULL OR body_enc IS NOT NULL);

-- The full-text search no longer indexes the body, which would otherwise stay readable in
-- the search vector.
DROP TRIGGER task_tsvector_update ON task;
DROP TRIGGER task_tsvector_insert ON task;
DROP INDEX task_textsearch_idx;
ALTER TABLE task RENAME COLUMN title_body_project_tags_tsv TO title_project_tags_tsv;

CREATE OR REPLACE FUNCTION task_trigger() RETURNS trigger AS $$
begin
  new.title_project_tags_tsv :=
    setweight(to_tsvector('pg_catalog.english', new.title), 'A') ||
    setweight(to_tsvector('pg_catalog.english', new.project), 'C') ||
    setweight(to_tsvector('pg_catalog.english', new.tags::text), 'D');
  return new;
end
$$ LANGUAGE plpgsql;

CREATE TRIGGER task_tsvector_update BEFORE
  UPDATE ON task
  FOR EACH ROW
    WHEN (
      OLD.title IS DISTINCT FROM NEW.title
      OR OLD.project IS DISTINCT FROM NEW.project
      OR OLD.tags IS DISTINCT FROM NEW.tags
    )
    EXECUTE FUNCTION task_trigger();

CREATE TRIGGER task_tsvector_insert BEFORE
  INSERT ON task
  FOR EACH ROW
    EXECUTE FUNCTION task_trigger();

UPDATE task
   SET title_project_tags_tsv =
       setweight(to_tsvector('english', title), 'A') ||
       setweight(to_tsvector('english', project), 'C') ||
       setweight(to_tsvector('english', tags::text), 'D');

CREATE INDEX task_textsearch_idx ON task USING GIN (title_project_tags_tsv);
