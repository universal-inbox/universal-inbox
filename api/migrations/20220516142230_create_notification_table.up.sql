-- Create notifications table
CREATE TABLE notification(
   id uuid NOT NULL,
   PRIMARY KEY (id),
   title TEXT NOT NULL,
   kind TEXT NOT NULL,
   status TEXT NOT NULL,
   metadata JSON NOT NULL,
   updated_at TIMESTAMP NOT NULL,
   last_read_at TIMESTAMP
);
