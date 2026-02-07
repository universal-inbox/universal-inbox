use chrono::{TimeZone, Utc};
use rstest::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{google_calendar::GoogleCalendarConfig, google_mail::GoogleMailConfig},
    },
    notification::{Notification, NotificationStatus, service::NotificationPatch},
    third_party::integrations::{
        google_calendar::GoogleCalendarEvent,
        google_mail::{GOOGLE_MAIL_INBOX_LABEL, GoogleMailThread},
    },
};

use universal_inbox_api::{configuration::Settings, integrations::oauth2::NangoConnection};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, nango_google_calendar_connection,
        nango_google_mail_connection,
    },
    notification::{
        google_calendar::{
            create_notification_from_google_calendar_event, google_calendar_event,
            mock_google_calendar_event_delete_service,
        },
        google_mail::{google_mail_thread_get_123, mock_google_mail_thread_modify_service},
    },
    rest::patch_resource,
    settings,
};

mod patch_resource {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_google_calendar_notification_status_as_deleted(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_thread_get_123: GoogleMailThread,
        google_calendar_event: GoogleCalendarEvent,
        nango_google_mail_connection: Box<NangoConnection>,
        nango_google_calendar_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;

        let google_mail_config = GoogleMailConfig::enabled();
        let synced_label_id = google_mail_config.synced_label.id.clone();
        let google_mail_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
            &settings,
            nango_google_mail_connection,
            None,
            None,
        )
        .await;

        let google_calendar_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
            &settings,
            nango_google_calendar_connection,
            None,
            None,
        )
        .await;

        let expected_notification = create_notification_from_google_calendar_event(
            &app.app,
            &google_mail_thread_get_123,
            &google_calendar_event,
            app.user.id,
            google_mail_integration_connection.id,
            google_calendar_integration_connection.id,
        )
        .await;

        // No call to the Google Calendar API should have been emitted
        // But the underlying Google mail is archived
        let gmail_third_party_item = expected_notification
            .source_item
            .source_item
            .clone()
            .unwrap();
        let google_mail_thread_modify_mock = mock_google_mail_thread_modify_service(
            &app.app.google_mail_mock_server,
            &gmail_third_party_item.source_id,
            vec![],
            vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label_id],
        );

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                status: NotificationStatus::Deleted,
                ..*expected_notification
            })
        );
        google_mail_thread_modify_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_google_calendar_notification_status_as_unsubscribed(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_thread_get_123: GoogleMailThread,
        google_calendar_event: GoogleCalendarEvent,
        nango_google_mail_connection: Box<NangoConnection>,
        nango_google_calendar_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;

        let google_mail_config = GoogleMailConfig::enabled();
        let synced_label_id = google_mail_config.synced_label.id.clone();
        let google_mail_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
            &settings,
            nango_google_mail_connection,
            None,
            None,
        )
        .await;

        let google_calendar_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
            &settings,
            nango_google_calendar_connection,
            None,
            None,
        )
        .await;

        let expected_notification = create_notification_from_google_calendar_event(
            &app.app,
            &google_mail_thread_get_123,
            &google_calendar_event,
            app.user.id,
            google_mail_integration_connection.id,
            google_calendar_integration_connection.id,
        )
        .await;

        // Unsubscribed notifications are archived on Google Mail
        // Universal Inbox will ignore new messages and archive them during the next sync
        // and the Google Calendar Event is deleted
        let gmail_third_party_item = expected_notification
            .source_item
            .source_item
            .clone()
            .unwrap();
        let google_mail_thread_modify_mock = mock_google_mail_thread_modify_service(
            &app.app.google_mail_mock_server,
            &gmail_third_party_item.source_id,
            vec![],
            vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label_id],
        );

        let google_calendar_event_delete_mock = mock_google_calendar_event_delete_service(
            &app.app.google_calendar_mock_server,
            &google_calendar_event.id,
        );

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Unsubscribed),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                status: NotificationStatus::Unsubscribed,
                ..*expected_notification
            })
        );
        google_mail_thread_modify_mock.assert();
        google_calendar_event_delete_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_google_calendar_notification_snoozed_until(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_thread_get_123: GoogleMailThread,
        google_calendar_event: GoogleCalendarEvent,
        nango_google_mail_connection: Box<NangoConnection>,
        nango_google_calendar_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;

        let google_mail_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleMail(GoogleMailConfig::enabled()),
            &settings,
            nango_google_mail_connection,
            None,
            None,
        )
        .await;

        let google_calendar_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
            &settings,
            nango_google_calendar_connection,
            None,
            None,
        )
        .await;

        let expected_notification = create_notification_from_google_calendar_event(
            &app.app,
            &google_mail_thread_get_123,
            &google_calendar_event,
            app.user.id,
            google_mail_integration_connection.id,
            google_calendar_integration_connection.id,
        )
        .await;
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.id.into(),
            &NotificationPatch {
                snoozed_until: Some(snoozed_time),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                snoozed_until: Some(snoozed_time),
                ..*expected_notification
            })
        );
    }
}

mod update_invitation {
    use universal_inbox::{
        notification::service::InvitationPatch,
        third_party::{
            integrations::google_calendar::{
                EventAttendee, GoogleCalendarEventAttendeeResponseStatus,
            },
            item::ThirdPartyItemData,
        },
    };

    use crate::helpers::{
        notification::google_calendar::mock_google_calendar_event_answer_service,
        rest::get_resource,
    };

    use super::*;

    #[rstest]
    #[case(GoogleCalendarEventAttendeeResponseStatus::Accepted)]
    #[case(GoogleCalendarEventAttendeeResponseStatus::Declined)]
    #[case(GoogleCalendarEventAttendeeResponseStatus::Tentative)]
    #[tokio::test]
    async fn test_answer_google_calendar_invitation(
        #[case] response_status: GoogleCalendarEventAttendeeResponseStatus,
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_thread_get_123: GoogleMailThread,
        google_calendar_event: GoogleCalendarEvent,
        nango_google_mail_connection: Box<NangoConnection>,
        nango_google_calendar_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;

        let google_mail_config = GoogleMailConfig::enabled();
        let synced_label_id = google_mail_config.synced_label.id.clone();
        let google_mail_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
            &settings,
            nango_google_mail_connection,
            None,
            None,
        )
        .await;

        let google_calendar_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleCalendar(GoogleCalendarConfig::enabled()),
            &settings,
            nango_google_calendar_connection,
            None,
            None,
        )
        .await;

        let expected_notification = create_notification_from_google_calendar_event(
            &app.app,
            &google_mail_thread_get_123,
            &google_calendar_event,
            app.user.id,
            google_mail_integration_connection.id,
            google_calendar_integration_connection.id,
        )
        .await;
        let gcal_third_party_item = expected_notification.source_item.clone();
        let gmail_third_party_item = expected_notification
            .source_item
            .source_item
            .clone()
            .unwrap();
        let google_mail_thread_modify_mock = mock_google_mail_thread_modify_service(
            &app.app.google_mail_mock_server,
            &gmail_third_party_item.source_id,
            vec![],
            vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label_id],
        );
        let mut attendees = google_calendar_event.attendees.clone();
        if let Some(idx) = attendees.iter().position(|a| a.self_ == Some(true)) {
            attendees[idx] = EventAttendee {
                response_status,
                ..attendees[idx].clone()
            };
        }
        let updated_google_calendar_event = GoogleCalendarEvent {
            attendees: attendees.clone(),
            ..google_calendar_event
        };
        let google_calendar_event_answer_mock = mock_google_calendar_event_answer_service(
            &app.app.google_calendar_mock_server,
            &gcal_third_party_item.source_id,
            attendees,
            &updated_google_calendar_event,
        );

        let patched_notification: Box<Notification> = app
            .client
            .patch(format!(
                "{}notifications/{}/invitation",
                app.app.api_address, expected_notification.id
            ))
            .json(&InvitationPatch { response_status })
            .send()
            .await
            .expect("Failed to execute request")
            .json()
            .await
            .expect("Cannot parse JSON result");

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                status: NotificationStatus::Deleted,
                ..*expected_notification
            })
        );
        google_mail_thread_modify_mock.assert();
        google_calendar_event_answer_mock.assert();

        let ThirdPartyItemData::GoogleCalendarEvent(event) = patched_notification.source_item.data
        else {
            unreachable!("notification's third party item must be a GoogleCalendarEvent");
        };
        assert_eq!(
            event.get_self_attendee().unwrap().response_status,
            response_status
        );
        let boxed_updated_google_calendar_event = Box::new(updated_google_calendar_event);
        assert_eq!(event, boxed_updated_google_calendar_event);

        let updated_notification: Box<Notification> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.id.into(),
        )
        .await;

        assert_eq!(
            updated_notification.source_item.data,
            ThirdPartyItemData::GoogleCalendarEvent(boxed_updated_google_calendar_event)
        );
    }
}
