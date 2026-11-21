use std::{path::Path, sync::Arc};

use super::default_php;
use crate::domain::{
    composer::{php_version_id, php_version_label, select_release, REQUIRED_EXTENSIONS},
    errors::ApplicationError,
    ports::{
        composer::{ComposerEnvironment, ComposerManagerPort, ComposerReleasesPort},
        php_ini::PhpIniStore,
        process_manager::{PhpDetectorPort, PhpInstallation},
    },
};

/// What the Composer section shows.
#[derive(Debug, Clone)]
pub struct ComposerStatusView {
    pub env: ComposerEnvironment,
    /// Major from Settings (`8.2`).
    pub default_php: String,
    /// The default PHP's install, if detected.
    pub php: Option<PhpInstallation>,
    /// Newest release that runs on the default PHP; `None` offline.
    pub latest: Option<String>,
    pub update_available: bool,
    /// Required extensions the default PHP does not load.
    pub missing_extensions: Vec<String>,
    /// Open Herd manages that PHP's php.ini, so it can enable them.
    pub can_enable_extensions: bool,
    /// The environment's PATH warnings plus version / PHP problems.
    pub warnings: Vec<String>,
}

pub struct GetComposerStatusUseCase {
    manager: Arc<dyn ComposerManagerPort>,
    releases: Arc<dyn ComposerReleasesPort>,
    detector: Arc<dyn PhpDetectorPort>,
    ini: Arc<dyn PhpIniStore>,
}

impl GetComposerStatusUseCase {
    pub fn new(
        manager: Arc<dyn ComposerManagerPort>,
        releases: Arc<dyn ComposerReleasesPort>,
        detector: Arc<dyn PhpDetectorPort>,
        ini: Arc<dyn PhpIniStore>,
    ) -> Self {
        Self { manager, releases, detector, ini }
    }

    pub async fn execute(&self, default_major: &str) -> Result<ComposerStatusView, ApplicationError> {
        let env = self.manager.environment().await?;
        let php = default_php(self.detector.as_ref(), default_major).await;
        let mut warnings = Vec::new();
        let php_id = php.as_ref().and_then(|p| php_version_id(&p.version));

        // getcomposer.org unreachable: the rest of the status is still useful.
        let latest = match (self.releases.releases().await, php_id) {
            (Ok(list), Some(id)) => select_release(&list, id).map(|r| r.version.clone()),
            (Ok(list), None) => list.first().map(|r| r.version.clone()),
            (Err(e), _) => { tracing::debug!("Composer release list: {e}"); None }
        };
        let installed_version = env.installed.as_ref().and_then(|i| i.version.clone());
        let update_available = env.installed.is_some() && latest.is_some() && latest != installed_version;

        let missing_extensions = match &php {
            Some(p) => self.missing_extensions(p, &mut warnings).await,
            None => Vec::new(),
        };
        let can_enable_extensions = !missing_extensions.is_empty()
            && matches!(self.ini.read(default_major).await, Ok(Some(_)));

        if env.installed.is_some() {
            warnings.extend(version_warnings(&env, default_major, php.as_ref()));
        }
        warnings.extend(env.warnings.iter().cloned());

        Ok(ComposerStatusView {
            env, default_php: default_major.to_string(), php, latest, update_available,
            missing_extensions, can_enable_extensions, warnings,
        })
    }

    async fn missing_extensions(&self, php: &PhpInstallation, warnings: &mut Vec<String>) -> Vec<String> {
        match self.manager.php_modules(Path::new(&php.binary_path)).await {
            Ok(modules) => REQUIRED_EXTENSIONS.iter()
                .filter(|ext| !modules.iter().any(|m| m == *ext))
                .map(|ext| (*ext).to_string())
                .collect(),
            Err(e) => {
                warnings.push(format!("Could not check the extensions of PHP {}: {e}", php.version));
                Vec::new()
            }
        }
    }
}

/// Problems with an installed Composer: no default PHP, a phar too new for
/// it, or a command still running another PHP.
fn version_warnings(env: &ComposerEnvironment, default_major: &str, php: Option<&PhpInstallation>) -> Vec<String> {
    let Some(php) = php else {
        return vec![format!("PHP {default_major} (the default version) is not installed, so `composer` cannot run: install it or pick another default in Settings.")];
    };
    let mut out = Vec::new();
    let min = env.installed.as_ref().and_then(|i| i.min_php);
    if let (Some(min), Some(id)) = (min, php_version_id(&php.version)) {
        if min > id {
            out.push(format!(
                "This Composer needs PHP {} or newer and the default PHP is {}: press Update to get the release that supports it.",
                php_version_label(min), php.version));
        }
    }
    if env.command_php.as_deref() != Some(Path::new(&php.binary_path)) {
        out.push(format!("The `composer` command does not use PHP {default_major} yet: press Repair to rewrite it."));
    }
    out
}
