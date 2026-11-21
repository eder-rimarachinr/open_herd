use crate::{
    application::php::{
        ini_parser::ExtensionChange,
        update_php_ini::{UpdatePhpIniCommand, UpdatePhpIniUseCase},
    },
    domain::{composer::REQUIRED_EXTENSIONS, errors::ApplicationError},
};

/// Turns on the extensions Composer needs in the default PHP's php.ini (only
/// for PHP versions Open Herd installed: 404 otherwise) and restarts that
/// PHP if it was running. Already enabled ones are left as they are.
pub struct EnableComposerExtensionsUseCase {
    update_ini: UpdatePhpIniUseCase,
}

impl EnableComposerExtensionsUseCase {
    pub fn new(update_ini: UpdatePhpIniUseCase) -> Self { Self { update_ini } }

    pub async fn execute(&self, default_major: &str) -> Result<(), ApplicationError> {
        let extension_changes = REQUIRED_EXTENSIONS.iter()
            .map(|name| ExtensionChange { name: (*name).to_string(), enabled: true })
            .collect();
        self.update_ini.execute(UpdatePhpIniCommand {
            major: default_major.to_string(), extension_changes, setting_changes: Vec::new(),
        }).await?;
        Ok(())
    }
}
