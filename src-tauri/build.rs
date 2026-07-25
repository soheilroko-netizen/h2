fn main() {
    tauri_build::build();

    #[cfg(windows)]
    {
        // request admin via linker manifest flag (no .res file, avoids VERSION conflict with Tauri)
        println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level=requireAdministrator");
    }
}
