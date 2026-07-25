fn main() {
    tauri_build::build();

    #[cfg(windows)]
    {
        embed_resource::compile("app.rc").expect("embed manifest");
    }
}
