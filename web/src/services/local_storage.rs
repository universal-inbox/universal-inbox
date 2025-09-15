use log::{debug, warn};

use crate::{settings::UserSettings, utils::get_local_storage};

pub struct LocalStorageService;

impl LocalStorageService {
    pub fn load_settings() -> UserSettings {
        let Ok(storage) = get_local_storage() else {
            warn!("Unable to access localStorage. Using default settings.");
            return UserSettings::default();
        };
        let Ok(Some(json)) = storage.get_item(UserSettings::STORAGE_KEY) else {
            debug!("No settings found in localStorage. Using defaults.");
            return UserSettings::default();
        };

        let Ok(settings) = UserSettings::from_json(&json) else {
            warn!("Failed to parse settings from localStorage. Using defaults.");
            return UserSettings::default();
        };

        debug!("Loaded settings from localStorage: {:?}", settings);
        settings
    }

    pub fn save_settings(settings: &UserSettings) {
        let Ok(storage) = get_local_storage() else {
            warn!("Unable to access localStorage. Settings not saved.");
            return;
        };
        let Ok(json) = settings.to_json() else {
            warn!("Failed to serialize settings, Settings not saved.");
            return;
        };

        if storage.set_item(UserSettings::STORAGE_KEY, &json).is_ok() {
            debug!("Settings saved to localStorage: {:?}", settings);
        } else {
            warn!("Failed to save settings to localStorage");
        }
    }

    pub fn update_ui_setting<F>(updater: F)
    where
        F: FnOnce(&mut crate::settings::UISettings),
    {
        let mut settings = Self::load_settings();
        updater(&mut settings.ui);
        Self::save_settings(&settings);
    }
}
