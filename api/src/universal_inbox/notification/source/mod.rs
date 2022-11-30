use clap::ArgEnum;
use macro_attr::macro_attr;
use serde::{Deserialize, Serialize};

macro_attr! {
    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Debug, EnumFromStr!, EnumDisplay!)]
    pub enum NotificationSourceKind {
        Github,
        Todoist
    }
}
