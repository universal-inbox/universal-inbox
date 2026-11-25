#![recursion_limit = "256"]
#[path = "../common/mod.rs"]
mod common;
#[allow(dead_code)]
mod helpers;
mod test_api_notification;
mod test_cache;
mod test_cron;
mod test_google_drive_notifications;
mod test_google_mail_notifications;
mod test_linear_tasks;
mod test_multi_task_manager;
mod test_oauth2_cimd;
mod test_slack_notifications;
mod test_slack_webhook;
mod test_todoist_notifications;
