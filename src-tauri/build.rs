// A build script reports failure by panicking: if tauri-build cannot run, the
// build must stop with its message, so `expect` is the intended behaviour here.
#[allow(clippy::expect_used)]
fn main() {
    let mut windows = tauri_build::WindowsAttributes::new();
    if let Ok(manifest) = std::fs::read_to_string("windows/app.manifest") {
        windows = windows.app_manifest(manifest);
    }

    tauri_build::try_build(
        tauri_build::Attributes::new().windows_attributes(windows)
    ).expect("failed to run tauri-build");
}
