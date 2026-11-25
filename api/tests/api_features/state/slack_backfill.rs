use universal_inbox::integration_connection::IntegrationConnectionId;
use universal_inbox_api::commands::slack::BackfillTeamIdReport;

#[derive(Default)]
pub struct SlackBackfill {
    pub mine: Option<IntegrationConnectionId>,
    pub others: Option<IntegrationConnectionId>,
    pub report: Option<BackfillTeamIdReport>,
}
