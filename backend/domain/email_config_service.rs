use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::email_service::EmailConfig;

pub struct EmailConfigService;

impl EmailConfigService {
    pub fn load_config(config_path: &Path) -> Result<EmailConfig> {
        let config_content = fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read email config file: {:?}", config_path))?;

        let config: EmailConfig = toml::from_str(&config_content)
            .with_context(|| "Failed to parse email config TOML")?;

        // Validate required fields
        if config.username.is_empty() {
            return Err(anyhow::anyhow!("Email username is required"));
        }
        if config.password.is_empty() {
            return Err(anyhow::anyhow!("Email password is required"));
        }
        if config.from_email.is_empty() {
            return Err(anyhow::anyhow!("From email is required"));
        }
        if config.to_emails.is_empty() {
            return Err(anyhow::anyhow!("At least one recipient email is required"));
        }

        Ok(config)
    }

    pub fn load_config_or_default(config_path: &Path) -> EmailConfig {
        match Self::load_config(config_path) {
            Ok(config) => config,
            Err(e) => {
                log::warn!("Failed to load email config from {:?}: {}", config_path, e);
                log::info!("Using default email config (notifications disabled)");
                EmailConfig::default()
            }
        }
    }
} 