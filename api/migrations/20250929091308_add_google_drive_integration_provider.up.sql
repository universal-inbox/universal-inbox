ALTER TYPE third_party_item_kind ADD VALUE IF NOT EXISTS 'GoogleDriveComment';

ALTER TYPE notification_source_kind ADD VALUE IF NOT EXISTS 'GoogleDrive';

ALTER TYPE integration_provider_kind ADD VALUE IF NOT EXISTS 'GoogleDrive';

ALTER TYPE integration_connection_config_kind ADD VALUE IF NOT EXISTS 'GoogleDrive';
