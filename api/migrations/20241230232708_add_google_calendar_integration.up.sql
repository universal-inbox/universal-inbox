ALTER TABLE third_party_item
ADD COLUMN source_item_id uuid,
ADD CONSTRAINT fk_source_item FOREIGN KEY (
    source_item_id
) REFERENCES third_party_item (id);

ALTER TYPE third_party_item_kind ADD VALUE IF NOT EXISTS 'GoogleCalendarEvent';

ALTER TYPE notification_source_kind ADD VALUE IF NOT EXISTS 'GoogleCalendar';

ALTER TYPE integration_provider_kind ADD VALUE IF NOT EXISTS 'GoogleCalendar';

ALTER TYPE integration_connection_config_kind ADD VALUE IF NOT EXISTS 'GoogleCalendar';
