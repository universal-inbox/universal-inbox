use anyhow::anyhow;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use typed_id::TypedId;
use url::Url;
use uuid::Uuid;

use crate::{
    HasHtmlUrl,
    integration_connection::IntegrationConnectionId,
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    user::UserId,
};

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct GoogleDriveComment {
    pub id: String,
    pub file_id: String,
    pub file_name: String,
    // application/vnd.google-apps.spreadsheet
    // application/vnd.google-apps.document
    pub file_mime_type: String,
    pub content: String,
    pub html_content: Option<String>,
    pub quoted_file_content: Option<String>,
    pub author: GoogleDriveCommentAuthor,
    pub created_time: DateTime<Utc>,
    pub modified_time: DateTime<Utc>,
    #[serde(default)]
    pub resolved: Option<bool>,
    pub replies: Vec<GoogleDriveCommentReply>,
    /// The email address of the current user (from IntegrationConnection context)
    #[serde(default)]
    pub user_email_address: Option<String>,
    /// The display name of the current user (from IntegrationConnection context)
    #[serde(default)]
    pub user_display_name: Option<String>,
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct GoogleDriveCommentAuthor {
    pub display_name: String,
    pub email_address: Option<String>,
    pub photo_link: Option<String>,
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct GoogleDriveCommentReply {
    pub id: String,
    pub content: String,
    pub html_content: Option<String>,
    pub author: GoogleDriveCommentAuthor,
    pub created_time: DateTime<Utc>,
    pub modified_time: DateTime<Utc>,
}

impl GoogleDriveComment {
    /// Check if the last reply in the comment was sent by the current user
    pub fn is_last_reply_from_user(&self) -> bool {
        let (Some(user_email), Some(user_display_name)) =
            (&self.user_email_address, &self.user_display_name)
        else {
            return false;
        };

        if let Some(latest_reply) = self.replies.last() {
            if let Some(ref latest_reply_author_email) = latest_reply.author.email_address
                && latest_reply_author_email == user_email
            {
                return true;
            }
            // Relying on weak display name match as email is not always available
            // https://issuetracker.google.com/issues/219879781
            if latest_reply.author.display_name == *user_display_name {
                return true;
            }
        }

        false
    }

    pub fn is_user_mentioned(
        &self,
        user_display_name: &str,
        user_email: &str,
        after_time: Option<DateTime<Utc>>,
    ) -> bool {
        // Check if the user is the author of the comment or any reply after the given time
        // without being the latest author
        if !self.replies.is_empty() {
            let latest_reply = self.replies[self.replies.len() - 1].clone();
            if let Some(ref latest_reply_author_email) = latest_reply.author.email_address
                && latest_reply_author_email == user_email
            {
                return false;
            }
            if latest_reply.author.display_name == user_display_name {
                return false;
            }
        }

        let is_new = after_time.is_none_or(|t| self.modified_time > t);
        if let Some(ref author_email) = self.author.email_address
            && is_new
            && author_email == user_email
        {
            return !self.replies.is_empty();
        }
        // Relying on weak display name match as email is not always available
        // https://issuetracker.google.com/issues/219879781
        if is_new && self.author.display_name == user_display_name {
            return !self.replies.is_empty();
        }

        if is_new && self.content.contains(user_email) {
            return true;
        }

        for reply in &self.replies {
            let is_new = after_time.is_none_or(|t| reply.modified_time > t);
            if let Some(ref reply_author_email) = reply.author.email_address
                && is_new
                && reply_author_email == user_email
            {
                return true;
            }

            // Relying on weak display name match as email is not always available
            // https://issuetracker.google.com/issues/219879781
            if is_new && reply.author.display_name == user_display_name {
                return true;
            }

            if is_new && reply.content.contains(user_email) {
                return true;
            }
        }

        false
    }
}

impl HasHtmlUrl for GoogleDriveComment {
    fn get_html_url(&self) -> Url {
        let base_path = match self.file_mime_type.as_str() {
            "application/vnd.google-apps.document" => "document",
            "application/vnd.google-apps.spreadsheet" => "spreadsheets",
            "application/vnd.google-apps.presentation" => "presentation",
            _ => "file",
        };
        Url::parse(&format!(
            "https://docs.google.com/{}/d/{}/edit?disco={}",
            base_path, self.file_id, self.id
        ))
        .unwrap()
    }
}

impl ThirdPartyItemFromSource for GoogleDriveComment {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        ThirdPartyItem {
            id: TypedId::new(Uuid::new_v4()),
            source_id: self.source_id(),
            data: ThirdPartyItemData::GoogleDriveComment(Box::new(self.clone())),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            user_id,
            integration_connection_id,
            source_item: None,
        }
    }

    fn source_id(&self) -> String {
        format!("{}#{}", self.file_id, self.id)
    }
}

impl TryFrom<ThirdPartyItem> for GoogleDriveComment {
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::GoogleDriveComment(comment) => Ok(*comment),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} to GoogleDriveComment",
                item.id
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rstest::*;

    #[fixture]
    fn comment_author() -> GoogleDriveCommentAuthor {
        GoogleDriveCommentAuthor {
            display_name: "John Doe".to_string(),
            email_address: Some("john.doe@example.com".to_string()),
            photo_link: Some("https://example.com/photo.jpg".to_string()),
        }
    }

    #[fixture]
    fn comment_reply(comment_author: GoogleDriveCommentAuthor) -> GoogleDriveCommentReply {
        GoogleDriveCommentReply {
            id: "reply_123".to_string(),
            content: "This is a reply".to_string(),
            html_content: Some("<p>This is a reply</p>".to_string()),
            author: comment_author,
            created_time: Utc.with_ymd_and_hms(2025, 9, 28, 10, 0, 0).unwrap(),
            modified_time: Utc.with_ymd_and_hms(2025, 9, 28, 10, 5, 0).unwrap(),
        }
    }

    #[fixture]
    fn google_drive_comment(
        comment_author: GoogleDriveCommentAuthor,
        comment_reply: GoogleDriveCommentReply,
    ) -> GoogleDriveComment {
        GoogleDriveComment {
            id: "comment_123".to_string(),
            file_id: "file_456".to_string(),
            file_name: "Test Document.docx".to_string(),
            file_mime_type: "application/vnd.google-apps.document".to_string(),
            content: "This is a test comment".to_string(),
            html_content: Some("<p>This is a test comment</p>".to_string()),
            quoted_file_content: Some("quoted text from document".to_string()),
            author: comment_author,
            created_time: Utc.with_ymd_and_hms(2025, 9, 28, 9, 0, 0).unwrap(),
            modified_time: Utc.with_ymd_and_hms(2025, 9, 28, 9, 30, 0).unwrap(),
            resolved: Some(false),
            replies: vec![comment_reply],
            user_email_address: None,
            user_display_name: None,
        }
    }

    #[rstest]
    fn test_google_drive_comment_in_document_html_url(google_drive_comment: GoogleDriveComment) {
        let html_url = google_drive_comment.get_html_url();

        assert_eq!(
            html_url.to_string(),
            "https://docs.google.com/document/d/file_456/edit?disco=comment_123"
        );
    }

    #[rstest]
    fn test_google_drive_comment_in_spreadsheet_html_url(
        mut google_drive_comment: GoogleDriveComment,
    ) {
        google_drive_comment.file_mime_type = "application/vnd.google-apps.spreadsheet".to_string();
        let html_url = google_drive_comment.get_html_url();

        assert_eq!(
            html_url.to_string(),
            "https://docs.google.com/spreadsheets/d/file_456/edit?disco=comment_123"
        );
    }

    #[rstest]
    fn test_google_drive_comment_in_presentation_html_url(
        mut google_drive_comment: GoogleDriveComment,
    ) {
        google_drive_comment.file_mime_type =
            "application/vnd.google-apps.presentation".to_string();
        let html_url = google_drive_comment.get_html_url();

        assert_eq!(
            html_url.to_string(),
            "https://docs.google.com/presentation/d/file_456/edit?disco=comment_123"
        );
    }

    #[rstest]
    fn test_google_drive_comment_in_file_html_url(mut google_drive_comment: GoogleDriveComment) {
        google_drive_comment.file_mime_type = "application/pdf".to_string();
        let html_url = google_drive_comment.get_html_url();

        assert_eq!(
            html_url.to_string(),
            "https://docs.google.com/file/d/file_456/edit?disco=comment_123"
        );
    }

    #[rstest]
    fn test_google_drive_comment_third_party_item_creation(
        google_drive_comment: GoogleDriveComment,
    ) {
        let user_id = UserId::from(Uuid::new_v4());
        let integration_connection_id = IntegrationConnectionId::from(Uuid::new_v4());

        let third_party_item =
            google_drive_comment.into_third_party_item(user_id, integration_connection_id);
        assert_eq!(third_party_item.source_id, "file_456#comment_123");
        assert_eq!(third_party_item.user_id, user_id);
        assert_eq!(
            third_party_item.integration_connection_id,
            integration_connection_id
        );

        match &third_party_item.data {
            ThirdPartyItemData::GoogleDriveComment(comment_data) => {
                assert_eq!(comment_data.id, "comment_123");
            }
            _ => panic!("Expected GoogleDriveComment data"),
        }
    }

    #[rstest]
    #[case::with_email_address(true)]
    #[case::without_email_address(false)]
    fn test_is_user_mentioned_as_author(#[case] has_email: bool) {
        let comment = GoogleDriveComment {
            id: "comment_123".to_string(),
            file_id: "file_456".to_string(),
            file_name: "Test Document.docx".to_string(),
            file_mime_type: "application/vnd.google-apps.document".to_string(),
            content: "Test comment".to_string(),
            html_content: None,
            quoted_file_content: None,
            author: GoogleDriveCommentAuthor {
                display_name: "Test User".to_string(),
                email_address: if has_email {
                    Some("test@example.com".to_string())
                } else {
                    None
                },
                photo_link: None,
            },
            created_time: Utc::now(),
            modified_time: Utc::now(),
            resolved: Some(false),
            replies: vec![GoogleDriveCommentReply {
                id: "reply_123".to_string(),
                content: "This is a reply".to_string(),
                html_content: None,
                author: GoogleDriveCommentAuthor {
                    display_name: "Other User".to_string(),
                    email_address: None,
                    photo_link: None,
                },
                created_time: Utc::now(),
                modified_time: Utc::now(),
            }],
            user_email_address: None,
            user_display_name: None,
        };

        assert!(comment.is_user_mentioned("Test User", "test@example.com", None));
        assert!(comment.is_user_mentioned(
            // mentioned after given time
            "Test User",
            "test@example.com",
            Some(Utc::now() - chrono::Duration::minutes(1))
        ));
        assert!(!comment.is_user_mentioned(
            // not mentioned after given time
            "Test User",
            "test@example.com",
            Some(Utc::now() + chrono::Duration::minutes(1))
        ));
        assert!(!comment.is_user_mentioned("Other User", "other@example.com", None));
    }

    #[rstest]
    fn test_is_user_mentioned_as_comment_author_without_reply() {
        let comment = GoogleDriveComment {
            id: "comment_123".to_string(),
            file_id: "file_456".to_string(),
            file_name: "Test Document.docx".to_string(),
            file_mime_type: "application/vnd.google-apps.document".to_string(),
            content: "Test comment".to_string(),
            html_content: None,
            quoted_file_content: None,
            author: GoogleDriveCommentAuthor {
                display_name: "Test User".to_string(),
                email_address: None,
                photo_link: None,
            },
            created_time: Utc::now(),
            modified_time: Utc::now(),
            resolved: Some(false),
            replies: vec![],
            user_email_address: None,
            user_display_name: None,
        };

        assert!(!comment.is_user_mentioned("Test User", "test@example.com", None));
    }

    #[rstest]
    #[case::with_email_address(true)]
    #[case::without_email_address(false)]
    fn test_is_user_mentioned_as_latest_reply_author(#[case] has_email: bool) {
        let comment = GoogleDriveComment {
            id: "comment_123".to_string(),
            file_id: "file_456".to_string(),
            file_name: "Test Document.docx".to_string(),
            file_mime_type: "application/vnd.google-apps.document".to_string(),
            content: "Test comment".to_string(),
            html_content: None,
            quoted_file_content: None,
            author: GoogleDriveCommentAuthor {
                display_name: "Test User".to_string(),
                email_address: if has_email {
                    Some("test@example.com".to_string())
                } else {
                    None
                },
                photo_link: None,
            },
            created_time: Utc::now(),
            modified_time: Utc::now(),
            resolved: Some(false),
            replies: vec![
                GoogleDriveCommentReply {
                    id: "reply_123".to_string(),
                    content: "This is a reply".to_string(),
                    html_content: None,
                    author: GoogleDriveCommentAuthor {
                        display_name: "Other User".to_string(),
                        email_address: None,
                        photo_link: None,
                    },
                    created_time: Utc::now(),
                    modified_time: Utc::now(),
                },
                GoogleDriveCommentReply {
                    id: "reply_123".to_string(),
                    content: "This is a reply".to_string(),
                    html_content: None,
                    author: GoogleDriveCommentAuthor {
                        display_name: "Test User".to_string(),
                        email_address: if has_email {
                            Some("test@example.com".to_string())
                        } else {
                            None
                        },
                        photo_link: None,
                    },
                    created_time: Utc::now(),
                    modified_time: Utc::now(),
                },
            ],
            user_email_address: None,
            user_display_name: None,
        };

        assert!(!comment.is_user_mentioned("Test User", "test@example.com", None));
    }

    #[rstest]
    fn test_is_user_mentioned_in_content() {
        let comment = GoogleDriveComment {
            id: "comment_456".to_string(),
            file_id: "file_789".to_string(),
            file_name: "Test Document.docx".to_string(),
            file_mime_type: "application/vnd.google-apps.document".to_string(),
            content: "Hey @test@example.com, please review this".to_string(),
            html_content: None,
            quoted_file_content: None,
            author: GoogleDriveCommentAuthor {
                display_name: "Other User".to_string(),
                email_address: Some("other@example.com".to_string()),
                photo_link: None,
            },
            created_time: Utc::now(),
            modified_time: Utc::now(),
            resolved: Some(false),
            replies: vec![],
            user_email_address: None,
            user_display_name: None,
        };

        assert!(comment.is_user_mentioned("Test User", "test@example.com", None));
        assert!(comment.is_user_mentioned(
            "Test User",
            "test@example.com",
            Some(Utc::now() - chrono::Duration::minutes(1))
        ));
        assert!(!comment.is_user_mentioned(
            "Test User",
            "test@example.com",
            Some(Utc::now() + chrono::Duration::minutes(1))
        ));
    }

    #[rstest]
    #[case::with_email_address(true)]
    #[case::without_email_address(false)]
    fn test_is_user_mentioned_as_reply_author(#[case] has_email: bool) {
        let comment = GoogleDriveComment {
            id: "comment_789".to_string(),
            file_id: "file_123".to_string(),
            file_name: "Test Document.docx".to_string(),
            file_mime_type: "application/vnd.google-apps.document".to_string(),
            content: "Random comment".to_string(),
            html_content: None,
            quoted_file_content: None,
            author: GoogleDriveCommentAuthor {
                display_name: "Other User".to_string(),
                email_address: if has_email {
                    Some("other@example.com".to_string())
                } else {
                    None
                },
                photo_link: None,
            },
            created_time: Utc::now(),
            modified_time: Utc::now(),
            resolved: Some(false),
            replies: vec![
                GoogleDriveCommentReply {
                    id: "reply_123".to_string(),
                    content: "This is a reply".to_string(),
                    html_content: None,
                    author: GoogleDriveCommentAuthor {
                        display_name: "Test User".to_string(),
                        email_address: if has_email {
                            Some("test@example.com".to_string())
                        } else {
                            None
                        },
                        photo_link: None,
                    },
                    created_time: Utc::now(),
                    modified_time: Utc::now(),
                },
                GoogleDriveCommentReply {
                    id: "reply_123".to_string(),
                    content: "This is another reply".to_string(),
                    html_content: None,
                    author: GoogleDriveCommentAuthor {
                        display_name: "Other User".to_string(),
                        email_address: None,
                        photo_link: None,
                    },
                    created_time: Utc::now(),
                    modified_time: Utc::now(),
                },
            ],
            user_email_address: None,
            user_display_name: None,
        };

        assert!(comment.is_user_mentioned("Test User", "test@example.com", None));
        assert!(comment.is_user_mentioned(
            "Test User",
            "test@example.com",
            Some(Utc::now() - chrono::Duration::minutes(1))
        ));
        assert!(!comment.is_user_mentioned(
            "Test User",
            "test@example.com",
            Some(Utc::now() + chrono::Duration::minutes(1))
        ));
    }

    #[rstest]
    fn test_is_user_mentioned_as_reply_mention() {
        let comment = GoogleDriveComment {
            id: "comment_789".to_string(),
            file_id: "file_123".to_string(),
            file_name: "Test Document.docx".to_string(),
            file_mime_type: "application/vnd.google-apps.document".to_string(),
            content: "Random comment".to_string(),
            html_content: None,
            quoted_file_content: None,
            author: GoogleDriveCommentAuthor {
                display_name: "Other User".to_string(),
                email_address: Some("other@example.com".to_string()),
                photo_link: None,
            },
            created_time: Utc::now(),
            modified_time: Utc::now(),
            resolved: Some(false),
            replies: vec![GoogleDriveCommentReply {
                id: "reply_123".to_string(),
                content: "Hey @test@example.com, please review this".to_string(),
                html_content: None,
                author: GoogleDriveCommentAuthor {
                    display_name: "Other User".to_string(),
                    email_address: Some("other@example.com".to_string()),
                    photo_link: None,
                },
                created_time: Utc::now(),
                modified_time: Utc::now(),
            }],
            user_email_address: None,
            user_display_name: None,
        };

        assert!(comment.is_user_mentioned("Test User", "test@example.com", None));
        assert!(comment.is_user_mentioned(
            "Test User",
            "test@example.com",
            Some(Utc::now() - chrono::Duration::minutes(1))
        ));
        assert!(!comment.is_user_mentioned(
            "Test User",
            "test@example.com",
            Some(Utc::now() + chrono::Duration::minutes(1))
        ));
    }

    #[rstest]
    fn test_is_last_reply_from_user_no_user_info() {
        let comment = GoogleDriveComment {
            id: "comment_123".to_string(),
            file_id: "file_456".to_string(),
            file_name: "Test Document.docx".to_string(),
            file_mime_type: "application/vnd.google-apps.document".to_string(),
            content: "Test comment".to_string(),
            html_content: None,
            quoted_file_content: None,
            author: GoogleDriveCommentAuthor {
                display_name: "Other User".to_string(),
                email_address: Some("other@example.com".to_string()),
                photo_link: None,
            },
            created_time: Utc::now(),
            modified_time: Utc::now(),
            resolved: Some(false),
            replies: vec![GoogleDriveCommentReply {
                id: "reply_123".to_string(),
                content: "This is a reply from user".to_string(),
                html_content: None,
                author: GoogleDriveCommentAuthor {
                    display_name: "Test User".to_string(),
                    email_address: Some("test@example.com".to_string()),
                    photo_link: None,
                },
                created_time: Utc::now(),
                modified_time: Utc::now(),
            }],
            // No user info stored on comment
            user_email_address: None,
            user_display_name: None,
        };

        assert!(!comment.is_last_reply_from_user());
    }

    #[rstest]
    #[case::with_email_address(true)]
    #[case::without_email_address(false)]
    fn test_is_last_reply_from_user_when_user_is_last_replier(#[case] has_email: bool) {
        let comment = GoogleDriveComment {
            id: "comment_123".to_string(),
            file_id: "file_456".to_string(),
            file_name: "Test Document.docx".to_string(),
            file_mime_type: "application/vnd.google-apps.document".to_string(),
            content: "Test comment".to_string(),
            html_content: None,
            quoted_file_content: None,
            author: GoogleDriveCommentAuthor {
                display_name: "Other User".to_string(),
                email_address: Some("other@example.com".to_string()),
                photo_link: None,
            },
            created_time: Utc::now(),
            modified_time: Utc::now(),
            resolved: Some(false),
            replies: vec![GoogleDriveCommentReply {
                id: "reply_123".to_string(),
                content: "This is a reply from user".to_string(),
                html_content: None,
                author: GoogleDriveCommentAuthor {
                    display_name: "Test User".to_string(),
                    email_address: if has_email {
                        Some("test@example.com".to_string())
                    } else {
                        None
                    },
                    photo_link: None,
                },
                created_time: Utc::now(),
                modified_time: Utc::now(),
            }],
            user_email_address: Some("test@example.com".to_string()),
            user_display_name: Some("Test User".to_string()),
        };

        assert!(comment.is_last_reply_from_user());
    }

    #[rstest]
    fn test_is_last_reply_from_user_when_other_is_last_replier() {
        let comment = GoogleDriveComment {
            id: "comment_123".to_string(),
            file_id: "file_456".to_string(),
            file_name: "Test Document.docx".to_string(),
            file_mime_type: "application/vnd.google-apps.document".to_string(),
            content: "Test comment".to_string(),
            html_content: None,
            quoted_file_content: None,
            author: GoogleDriveCommentAuthor {
                display_name: "Other User".to_string(),
                email_address: Some("other@example.com".to_string()),
                photo_link: None,
            },
            created_time: Utc::now(),
            modified_time: Utc::now(),
            resolved: Some(false),
            replies: vec![
                GoogleDriveCommentReply {
                    id: "reply_123".to_string(),
                    content: "User reply".to_string(),
                    html_content: None,
                    author: GoogleDriveCommentAuthor {
                        display_name: "Test User".to_string(),
                        email_address: Some("test@example.com".to_string()),
                        photo_link: None,
                    },
                    created_time: Utc::now(),
                    modified_time: Utc::now(),
                },
                GoogleDriveCommentReply {
                    id: "reply_456".to_string(),
                    content: "Other reply".to_string(),
                    html_content: None,
                    author: GoogleDriveCommentAuthor {
                        display_name: "Other User".to_string(),
                        email_address: Some("other@example.com".to_string()),
                        photo_link: None,
                    },
                    created_time: Utc::now(),
                    modified_time: Utc::now(),
                },
            ],
            user_email_address: Some("test@example.com".to_string()),
            user_display_name: Some("Test User".to_string()),
        };

        assert!(!comment.is_last_reply_from_user());
    }

    #[rstest]
    fn test_is_last_reply_from_user_with_no_replies() {
        let comment = GoogleDriveComment {
            id: "comment_123".to_string(),
            file_id: "file_456".to_string(),
            file_name: "Test Document.docx".to_string(),
            file_mime_type: "application/vnd.google-apps.document".to_string(),
            content: "Test comment".to_string(),
            html_content: None,
            quoted_file_content: None,
            author: GoogleDriveCommentAuthor {
                display_name: "Test User".to_string(),
                email_address: Some("test@example.com".to_string()),
                photo_link: None,
            },
            created_time: Utc::now(),
            modified_time: Utc::now(),
            resolved: Some(false),
            replies: vec![],
            user_email_address: Some("test@example.com".to_string()),
            user_display_name: Some("Test User".to_string()),
        };

        assert!(!comment.is_last_reply_from_user());
    }
}
