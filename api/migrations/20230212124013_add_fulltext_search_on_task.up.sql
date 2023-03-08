CREATE FUNCTION task_trigger() RETURNS trigger AS $$
begin
  new.title_body_project_tags_tsv :=
    setweight(to_tsvector('pg_catalog.english', new.title), 'A') ||
    setweight(to_tsvector('pg_catalog.english', new.body), 'B') ||
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
      OR OLD.body IS DISTINCT FROM NEW.body
      OR OLD.project IS DISTINCT FROM NEW.project
      OR OLD.tags IS DISTINCT FROM NEW.tags
    )
    EXECUTE FUNCTION task_trigger();

CREATE TRIGGER task_tsvector_insert BEFORE
  INSERT ON task
  FOR EACH ROW
    EXECUTE FUNCTION task_trigger();

ALTER TABLE task
  ADD COLUMN title_body_project_tags_tsv tsvector;

CREATE INDEX task_textsearch_idx ON task USING GIN (title_body_project_tags_tsv);

UPDATE task
   SET title_body_project_tags_tsv =
       setweight(to_tsvector('english', title), 'A') ||
       setweight(to_tsvector('english', body), 'B') ||
       setweight(to_tsvector('english', project), 'C') ||
       setweight(to_tsvector('english', tags::text), 'D');

