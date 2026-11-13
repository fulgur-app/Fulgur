// Build script to embed the Windows icon into the executable.
//
// `embed_resource::compile` checks the compilation target itself and returns
// `NotWindows` for other targets, so it runs unconditionally. A host-side
// `#[cfg(target_os = "windows")]` would describe the build machine instead of
// the target and skip the icon when cross-compiling for Windows.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/file_icon.ico");
    println!("cargo:rerun-if-changed=resources/windows/app.rc");
    if let Err(error) = embed_resource::compile("resources/windows/app.rc", embed_resource::NONE)
        .manifest_optional()
    {
        panic!("failed to embed the Windows icon resource: {error}");
    }
}
