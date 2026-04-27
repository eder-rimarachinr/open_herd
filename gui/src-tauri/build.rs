fn main() {
    let mut windows = tauri_build::WindowsAttributes::new();
    // Use our custom manifest for elevation
    if let Ok(manifest) = std::fs::read_to_string("windows/app.manifest") {
        windows = windows.app_manifest(manifest);
    }
    
    tauri_build::try_build(
        tauri_build::Attributes::new().windows_attributes(windows)
    ).expect("failed to run tauri-build");
}
