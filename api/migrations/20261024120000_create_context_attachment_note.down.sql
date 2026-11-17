DROP TABLE attachment;

-- `context` and `note` reference each other
ALTER TABLE context DROP CONSTRAINT context_note_id_fkey;
DROP TABLE note;
DROP TABLE context;

DROP TYPE attachment_kind;
DROP TYPE context_status;
