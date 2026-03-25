pub mod config;
pub mod list_item;
pub mod notification_list_item;
pub mod preview;
pub mod task_list_item;

use universal_inbox::third_party::integrations::ticktick::TickTickTag;

use crate::components::Tag;

/// Map a TickTick tag onto the shared `Tag` enum used by `TagDisplay` /
/// `TagList`. Tags with a color persisted from the TickTick API render
/// in the source-native color used by the redesigned preview pane; tags
/// without color metadata fall through to the muted default style.
impl From<TickTickTag> for Tag {
    fn from(tag: TickTickTag) -> Self {
        match tag.color {
            Some(color) => Tag::Colored {
                name: tag.name,
                color: color.trim_start_matches('#').to_string(),
            },
            None => Tag::Default { name: tag.name },
        }
    }
}
