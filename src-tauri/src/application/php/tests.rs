#![cfg(test)]

use std::sync::{Arc, Mutex};
use async_trait::async_trait;

use crate::domain::{
    errors::{ApplicationError, DomainError, InfrastructureError},
    ports::{
        php_ini::{PhpIniFile, PhpIniStore},
        process_manager::{PhpDetectorPort, PhpInstallation, PhpProcessPort},
        web_server::WebServerPort,
    },
    site::entity::Site,
};
use super::{
    get_php_ini::GetPhpIniUseCase,
    ini_parser::{ExtensionChange, SettingChange},
    update_php_ini::{UpdatePhpIniCommand, UpdatePhpIniUseCase},
};

// ── Doubles ───────────────────────────────────────────────────────────────────

#[derive(Default)]
struct MemoryIni { content: Mutex<Option<String>>, writes: Mutex<u32> }

impl MemoryIni {
    fn with(content: &str) -> Arc<Self> {
        Arc::new(Self { content: Mutex::new(Some(content.into())), writes: Mutex::new(0) })
    }
}

#[async_trait]
impl PhpIniStore for MemoryIni {
    async fn read(&self, _major: &str) -> Result<Option<PhpIniFile>, InfrastructureError> {
        Ok(self.content.lock().unwrap().clone().map(|content| PhpIniFile { path: "/php/8.2/php.ini".into(), content }))
    }
    async fn write(&self, _major: &str, content: &str) -> Result<(), InfrastructureError> {
        *self.content.lock().unwrap() = Some(content.into());
        *self.writes.lock().unwrap() += 1;
        Ok(())
    }
}

/// Records the calls made to PHP / nginx, in order.
#[derive(Default)]
struct Calls { running: bool, log: Mutex<Vec<String>> }

impl Calls {
    fn running(running: bool) -> Arc<Self> { Arc::new(Self { running, log: Mutex::default() }) }
    fn push(&self, call: &str) { self.log.lock().unwrap().push(call.into()); }
    fn log(&self) -> Vec<String> { self.log.lock().unwrap().clone() }
}

#[async_trait]
impl PhpProcessPort for Calls {
    async fn start(&self, i: &PhpInstallation) -> Result<(), InfrastructureError> { self.push(&format!("start {}", i.major)); Ok(()) }
    async fn stop(&self, major: &str) -> Result<(), InfrastructureError> { self.push(&format!("stop {major}")); Ok(()) }
    async fn stop_all(&self) -> Result<(), InfrastructureError> { Ok(()) }
    async fn is_running(&self, _major: &str) -> bool { self.running }
    async fn running_majors(&self) -> Vec<String> { vec![] }
}

#[async_trait]
impl WebServerPort for Calls {
    async fn create_vhost(&self, _: &Site) -> Result<(), InfrastructureError> { Ok(()) }
    async fn remove_vhost(&self, _: &Site) -> Result<(), InfrastructureError> { Ok(()) }
    async fn reload(&self) -> Result<(), InfrastructureError> { self.push("reload nginx"); Ok(()) }
    async fn is_running(&self) -> bool { true }
}

#[async_trait]
impl PhpDetectorPort for Calls {
    async fn detect(&self) -> Vec<PhpInstallation> {
        vec![PhpInstallation { major: "8.2".into(), version: "8.2.31".into(), binary_path: "php.exe".into() }]
    }
}

fn update_uc(store: Arc<MemoryIni>, calls: Arc<Calls>) -> UpdatePhpIniUseCase {
    UpdatePhpIniUseCase::new(store, calls.clone(), calls.clone(), calls)
}

fn cmd(ext: &[(&str, bool)], settings: &[(&str, &str)]) -> UpdatePhpIniCommand {
    UpdatePhpIniCommand {
        major: "8.2".into(),
        extension_changes: ext.iter().map(|(n, e)| ExtensionChange { name: (*n).into(), enabled: *e }).collect(),
        setting_changes: settings.iter().map(|(k, v)| SettingChange { key: (*k).into(), value: (*v).into() }).collect(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn reading_a_version_without_php_ini_is_not_found() {
    let err = GetPhpIniUseCase::new(Arc::new(MemoryIni::default())).execute("8.2").await.unwrap_err();
    assert!(matches!(err, ApplicationError::Domain(DomainError::PhpIniNotFound(m)) if m == "8.2"));
}

#[tokio::test]
async fn invalid_change_is_rejected_before_touching_the_file() {
    let store = MemoryIni::with("memory_limit = 128M\n");
    let err = update_uc(store.clone(), Calls::running(true))
        .execute(cmd(&[], &[("memory_limit", "128M\nauto_prepend_file=/evil.php")])).await.unwrap_err();
    assert!(matches!(err, ApplicationError::Domain(DomainError::InvalidPhpIniChange(_))));
    assert_eq!(*store.writes.lock().unwrap(), 0, "nothing may be written");
}

#[tokio::test]
async fn valid_change_is_saved_and_returned() {
    let store = MemoryIni::with(";extension=intl\nmemory_limit = 128M\n");
    let view = update_uc(store.clone(), Calls::running(false))
        .execute(cmd(&[("intl", true)], &[("memory_limit", "512M")])).await.unwrap();

    assert_eq!(store.content.lock().unwrap().as_deref(), Some("extension=intl\nmemory_limit = 512M\n"));
    assert!(view.extensions.iter().any(|e| e.name == "intl" && e.enabled));
    assert!(view.settings.iter().any(|s| s.key == "memory_limit" && s.value == "512M"));
    assert_eq!(view.path, "/php/8.2/php.ini");
}

#[tokio::test]
async fn running_php_is_restarted_around_an_nginx_reload() {
    let calls = Calls::running(true);
    update_uc(MemoryIni::with("memory_limit = 128M\n"), calls.clone())
        .execute(cmd(&[], &[("memory_limit", "256M")])).await.unwrap();
    assert_eq!(calls.log(), ["stop 8.2", "reload nginx", "start 8.2"]);
}

#[tokio::test]
async fn stopped_php_is_left_stopped() {
    let calls = Calls::running(false);
    update_uc(MemoryIni::with("memory_limit = 128M\n"), calls.clone())
        .execute(cmd(&[], &[("memory_limit", "256M")])).await.unwrap();
    assert!(calls.log().is_empty());
}
