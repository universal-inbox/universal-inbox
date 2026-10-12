DROP TRIGGER audit_task_delete ON task;
DROP TRIGGER audit_task_update ON task;
DROP TRIGGER audit_task_insert ON task;
DROP FUNCTION audit_task_change();

DROP TRIGGER audit_notification_delete ON notification;
DROP TRIGGER audit_notification_update ON notification;
DROP TRIGGER audit_notification_insert ON notification;
DROP FUNCTION audit_notification_change();

DROP FUNCTION audit_change(ANYELEMENT, ANYELEMENT);
DROP FUNCTION audit_actor(TEXT, TEXT);

DROP TABLE audit_event;
DROP TYPE audit_entity_kind;
