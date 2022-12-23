CREATE TYPE old_task_status AS ENUM ('Active', 'Done');
ALTER TABLE task
  ALTER COLUMN status
  SET DATA TYPE OLD_TASK_STATUS
  USING (
    CASE status::TEXT
      WHEN 'Done'::TEXT THEN 'Done'::OLD_TASK_STATUS
      WHEN 'Deleted'::TEXT THEN 'Done'::OLD_TASK_STATUS
      ELSE 'Active'::OLD_TASK_STATUS
    END
  );
DROP TYPE task_status;
ALTER TYPE old_task_status RENAME TO task_status;
