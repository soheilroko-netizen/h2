fn main() {
    #[cfg(windows)]
    {
        // Statically link VC++ CRT → no side-by-side dependency
        println!("cargo:rustc-link-arg-bins=-Ctarget-feature=+crt-static");

        // Custom manifest: admin + Common Controls 6
        // No CRT dep needed with static linking above
        let manifest = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="stls" version="5.0.0.0" processorArchitecture="amd64"/>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccfdf1" language="*"/>
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#;
        let mut windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        windows = windows.app_manifest(manifest);
        let attrs = tauri_build::Attributes::new().windows_attributes(windows);
        tauri_build::try_build(attrs).expect("failed to run build script");
    }

    #[cfg(not(windows))]
    tauri_build::build();
}
