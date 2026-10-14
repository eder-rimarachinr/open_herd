use std::sync::Arc;
use crate::domain::{
    errors::{ApplicationError, DomainError},
    ports::php_ini::{PhpIniFile, PhpIniStore},
};
use super::ini_parser::{self, ExtensionState, SettingValue};

/// The editable part of a php.ini, as shown in the PHP page.
#[derive(Debug, Clone)]
pub struct PhpIniView {
    pub major: String,
    pub path: String,
    pub extensions: Vec<ExtensionState>,
    pub settings: Vec<SettingValue>,
}

impl PhpIniView {
    pub(super) fn of(major: &str, file: &PhpIniFile) -> Self {
        Self {
            major: major.to_owned(),
            path: file.path.clone(),
            extensions: ini_parser::parse_extensions(&file.content),
            settings: ini_parser::parse_settings(&file.content),
        }
    }
}

pub struct GetPhpIniUseCase {
    store: Arc<dyn PhpIniStore>,
}

impl GetPhpIniUseCase {
    pub fn new(store: Arc<dyn PhpIniStore>) -> Self { Self { store } }

    pub async fn execute(&self, major: &str) -> Result<PhpIniView, ApplicationError> {
        let file = self.store.read(major).await?
            .ok_or_else(|| DomainError::PhpIniNotFound(major.to_owned()))?;
        Ok(PhpIniView::of(major, &file))
    }
}
