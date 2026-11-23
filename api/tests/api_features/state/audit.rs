use universal_inbox::{
    Page,
    audit::{AuditEvent, AuditEventGroup},
    task::Task,
};

#[derive(Default)]
pub struct Audit {
    pub page: Option<Page<AuditEvent>>,
    pub second_page: Option<Page<AuditEvent>>,
    pub groups: Option<Page<AuditEventGroup>>,
    pub second_groups: Option<Page<AuditEventGroup>>,
    pub secret_description: Option<String>,
    pub task: Option<Task>,
    pub task_audit_body: Option<String>,
    pub purged: Option<u64>,
}

impl Audit {
    pub fn events(&self) -> &[AuditEvent] {
        &self
            .page
            .as_ref()
            .expect("No audit events were listed")
            .content
    }
}
