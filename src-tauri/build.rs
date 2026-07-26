fn main() {
    // Keep Tauri's default manifest (has all CRT/VC++ deps — no SxS errors)
    // Add admin elevation via explicit linker flag
    #[cfg(windows)]
    {
        let windows = tauri_build::WindowsAttributes::new();
        let attrs = tauri_build::Attributes::new().windows_attributes(windows);
        tauri_build::try_build(attrs).expect("tauri build failed");
        println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level=requireAdministrator");
        println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:uiAccess=false");
    }

    #[cfg(not(windows))]
    tauri_build::build();
}
