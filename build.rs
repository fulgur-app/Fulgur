// Build script to embed Windows icon into executable
fn main() {
    #[cfg(target_os = "windows")]
    {
        // Embed the icon resource on Windows
        let _ = embed_resource::compile("resources/windows/app.rc", embed_resource::NONE);
    }
}
