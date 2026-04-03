// Build script to embed Windows icon into executable
fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        println!("cargo:rerun-if-changed=assets/file_icon.ico");
        println!("cargo:rerun-if-changed=resources/windows/app.rc");
        // Embed the icon resource on Windows
        let _ = embed_resource::compile("resources/windows/app.rc", embed_resource::NONE);
    }
}
