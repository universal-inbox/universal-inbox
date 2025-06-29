ALTER TYPE third_party_item_kind ADD VALUE IF NOT EXISTS 'WebPage';

ALTER TYPE notification_source_kind ADD VALUE IF NOT EXISTS 'API';

ALTER TYPE integration_provider_kind ADD VALUE IF NOT EXISTS 'API';

ALTER TYPE integration_connection_config_kind ADD VALUE IF NOT EXISTS 'API';
