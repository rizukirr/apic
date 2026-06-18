//! Build script: embeds the application icon into the Windows executable so it
//! shows in Explorer, the title bar, and the taskbar. A no-op on every other
//! platform (the block is compiled only when the build host is Windows, which is
//! where the Windows release artifact is produced).
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            // Don't fail the build if the resource compiler is unavailable; the
            // binary still works, it just won't carry the embedded icon.
            println!("cargo:warning=failed to embed Windows icon: {e}");
        }
    }
}
