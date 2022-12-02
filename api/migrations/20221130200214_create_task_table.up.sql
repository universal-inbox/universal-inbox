-- Create some enumerations for the `task` table
CREATE TYPE task_status AS ENUM ('Active', 'Done');
CREATE TYPE task_kind AS ENUM ('Todoist');

-- Create a cast function from TEXT to TASK_KIND and mark it as immutable
-- to be used in a generated column (direct cast is not considered immutable)
CREATE FUNCTION text_to_task_kind(kind TEXT) RETURNS TASK_KIND
    IMMUTABLE
    RETURN kind::TASK_KIND;

CREATE TABLE task(
   id UUID NOT NULL,
   PRIMARY KEY (id),
   source_id TEXT NOT NULL,
   kind TASK_KIND GENERATED ALWAYS AS (text_to_task_kind(metadata->>'type')) STORED,
   CONSTRAINT task_source_id_kind_key UNIQUE (source_id, kind),
   title TEXT NOT NULL,
   body TEXT NOT NULL,
   status TASK_STATUS NOT NULL,
   completed_at TIMESTAMP,
   CONSTRAINT completed_at_not_null_when_done CHECK (status != 'Done' OR completed_at IS NOT NULL),
   priority INTEGER NOT NULL
   CONSTRAINT priority_range CHECK (priority <@ '[1, 4]'::int4range),
   due_at TIMESTAMP,
   source_html_url TEXT,
   tags TEXT[] NOT NULL,
   parent_id UUID,
   CONSTRAINT fk_parent FOREIGN KEY(parent_id) REFERENCES task(id),
   project TEXT NOT NULL,
   is_recurring BOOLEAN NOT NULL,
   created_at TIMESTAMP NOT NULL,
   metadata JSON NOT NULL
);

CREATE INDEX task_source_id_kind_idx
  ON task(source_id, kind);
