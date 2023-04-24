CREATE TYPE integration_connection_status AS ENUM ('Created', 'Validated', 'Failing');
CREATE TYPE integration_provider_kind AS ENUM ('Github', 'Todoist');

CREATE TABLE integration_connection(
  id UUID NOT NULL,
  PRIMARY KEY (id),
  connection_id UUID NOT NULL UNIQUE,
  user_id UUID NOT NULL,
  provider_kind INTEGRATION_PROVIDER_KIND NOT NULL,
  status INTEGRATION_CONNECTION_STATUS NOT NULL,
  failure_message TEXT,
  created_at TIMESTAMP NOT NULL,
  updated_at TIMESTAMP NOT NULL
)
