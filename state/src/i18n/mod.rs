//! Internationalization (i18n) module for UI localization.
//!
//! This module provides a lightweight, file-based translation system
//! that separates engine logic (pure English) from UI presentation
//! (localized to player's language, defaulting to Polish).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Locale configuration loaded from external JSON files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Locale {
    /// UI strings organized by category.
    pub ui: HashMap<String, HashMap<String, String>>,
    /// Sector name translations.
    pub sectors: HashMap<String, String>,
    /// Commodity name translations.
    pub commodities: HashMap<String, String>,
    /// Labor tier translations.
    pub labor_tiers: HashMap<String, String>,
    /// Wealth bracket translations.
    pub wealth_brackets: HashMap<String, String>,
    /// Regime type translations.
    pub regime_types: HashMap<String, String>,
}

/// Global i18n manager for runtime translation.
pub struct I18nManager {
    /// Current active locale (default: "pl").
    current_locale: String,
    /// Loaded locale data.
    locales: HashMap<String, Locale>,
}

impl I18nManager {
    /// Load all locale files from the locales directory.
    pub fn load_locales(locales_dir: &std::path::Path) -> Result<Self, String> {
        let mut locales = HashMap::new();
        
        // Load pl.json (default)
        let pl_path = locales_dir.join("pl.json");
        let pl_content = std::fs::read_to_string(&pl_path)
            .map_err(|e| format!("Failed to read pl.json: {}", e))?;
        let pl_locale: Locale = serde_json::from_str(&pl_content)
            .map_err(|e| format!("Failed to parse pl.json: {}", e))?;
        locales.insert("pl".to_string(), pl_locale);
        
        // Load en.json
        let en_path = locales_dir.join("en.json");
        if en_path.exists() {
            let en_content = std::fs::read_to_string(&en_path)
                .map_err(|e| format!("Failed to read en.json: {}", e))?;
            let en_locale: Locale = serde_json::from_str(&en_content)
                .map_err(|e| format!("Failed to parse en.json: {}", e))?;
            locales.insert("en".to_string(), en_locale);
        }
        
        Ok(Self {
            current_locale: "pl".to_string(),
            locales,
        })
    }
    
    /// Set the active locale.
    pub fn set_locale(&mut self, locale: &str) -> Result<(), String> {
        if self.locales.contains_key(locale) {
            self.current_locale = locale.to_string();
            Ok(())
        } else {
            Err(format!("Locale '{}' not found", locale))
        }
    }
    
    /// Get a translated UI string.
    pub fn t(&self, category: &str, key: &str) -> String {
        self.locales.get(&self.current_locale)
            .and_then(|locale| locale.ui.get(category))
            .and_then(|category_map| category_map.get(key))
            .cloned()
            .unwrap_or_else(|| format!("{}.{}", category, key))
    }
    
    /// Get a translated sector name.
    pub fn sector(&self, sector_key: &str) -> String {
        self.locales.get(&self.current_locale)
            .and_then(|locale| locale.sectors.get(sector_key))
            .cloned()
            .unwrap_or_else(|| sector_key.to_string())
    }
    
    /// Get a translated commodity name.
    pub fn commodity(&self, commodity_key: &str) -> String {
        self.locales.get(&self.current_locale)
            .and_then(|locale| locale.commodities.get(commodity_key))
            .cloned()
            .unwrap_or_else(|| commodity_key.to_string())
    }
}

/// Global i18n manager instance using OnceLock for thread-safe initialization.
static I18N_MANAGER: OnceLock<I18nManager> = OnceLock::new();

/// Initialize the global i18n manager (thread-safe).
pub fn init_i18n(locales_dir: &std::path::Path) -> Result<(), String> {
    let manager = I18nManager::load_locales(locales_dir)?;
    I18N_MANAGER.set(manager)
        .map_err(|_| "i18n already initialized".to_string())
}

/// Get the global i18n manager (thread-safe).
pub fn i18n() -> &'static I18nManager {
    I18N_MANAGER.get()
        .expect("i18n not initialized. Call init_i18n() first.")
}

/// Convenience macro for UI translations.
#[macro_export]
macro_rules! t {
    ($category:expr, $key:expr) => {
        $crate::i18n::i18n().t($category, $key)
    };
}
