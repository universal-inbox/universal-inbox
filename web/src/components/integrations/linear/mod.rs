pub mod config;
pub mod icons;
pub mod list_item;
pub mod notification;
pub mod notification_list_item;
pub mod preview;
pub mod task_list_item;

pub fn get_notification_type_label(notification_type: &str) -> String {
    match notification_type {
        "issueAddedToTriage" => "Added To Triage".to_string(),
        "issueAddedToView" => "Added To View".to_string(),
        "issueAssignedToYou" => "Assigned To You".to_string(),
        "issueBlocking" => "Blocked".to_string(),
        "issueCommentMention" => "Comment Mention".to_string(),
        "issueCommentReaction" => "Comment Reaction".to_string(),
        "issueCreated" => "Created".to_string(),
        "issueDue" => "Due".to_string(),
        "issueEmojiReaction" => "Reaction".to_string(),
        "issueMention" => "Mention".to_string(),
        "issueNewComment" => "New Comment".to_string(),
        "issueStatusChanged" => "Status Changed".to_string(),
        "issueUnassignedFromYou" => "Unassigned From You".to_string(),
        "projectAddedAsLead" => "Added As Lead".to_string(),
        "projectAddedAsMember" => "Added As Member".to_string(),
        "projectUpdateCreated" => "Update Created".to_string(),
        "projectUpdateMentionPrompt" => "Update Mention".to_string(),
        _ => notification_type.to_string(),
    }
}
