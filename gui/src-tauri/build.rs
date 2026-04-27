fn main() {
    // Embed the Windows manifest that requests administrator elevation.
    // Windows shows UAC once on launch; the app and all child processes
    // (including the daemon sidecar) inherit the elevated token.
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_manifest_file("windows/app.manifest");
        if let Err(e) = res.compile() {
            eprintln!("cargo:warning=Failed to embed Windows manifest: {e}");
        }
    }
    tauri_build::build()
}
