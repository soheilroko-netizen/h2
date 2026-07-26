fn main() {
    tauri_build::build();

    #[cfg(windows)]
    {
        // Add admin elevation via linker flag — keeps Tauri's default manifest
        // (VC++ CRT, UCRT, etc.) intact, just adds requireAdministrator.
        println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level=requireAdministrator");
    }
}
