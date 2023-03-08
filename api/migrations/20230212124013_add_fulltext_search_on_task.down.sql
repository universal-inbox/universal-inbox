DROP INDEX task_textsearch_idx;

ALTER TABLE task
  DROP COLUMN title_body_project_tags_tsv;

DROP TRIGGER task_tsvector_update ON task;
DROP TRIGGER task_tsvector_insert ON task;

DROP FUNCTION task_trigger;


