use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub ui: UISettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UISettings {
    pub details_panel_width: f64,
    pub details_panel_height: f64,
    pub details_panel_position: PanelPosition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum PanelPosition {
    #[default]
    Right,
    Bottom,
}

impl Default for UISettings {
    fn default() -> Self {
        Self {
            details_panel_width: 33.333, // Start with 1/3 width (33.333%)
            details_panel_height: 40.0,  // Start with 40% height
            details_panel_position: PanelPosition::default(),
        }
    }
}

impl UserSettings {
    pub const STORAGE_KEY: &'static str = "universal-inbox-settings";

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
