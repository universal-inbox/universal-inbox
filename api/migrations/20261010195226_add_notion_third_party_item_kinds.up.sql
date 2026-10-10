ALTER TYPE third_party_item_kind ADD VALUE IF NOT EXISTS 'NotionComment';

ALTER TYPE third_party_item_kind ADD VALUE IF NOT EXISTS 'NotionBlock';

ALTER TYPE notification_source_kind ADD VALUE IF NOT EXISTS 'Notion';
