use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let manifest = root.join("app.manifest");
    let icon = root.join("assets/branding/lumi.ico");
    for path in [&manifest, &icon] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let author = env::var("CARGO_PKG_AUTHORS").unwrap();
    let numeric = version.replace('.', ",") + ",0";
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let source = out.join("lumi.rc");
    let resource = out.join("lumi.res");
    fs::write(
        &source,
        format!(
            r#"
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "{author}"
      VALUE "FileDescription", "Lumi"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "lumi"
      VALUE "OriginalFilename", "lumi.exe"
      VALUE "ProductName", "Lumi"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#,
            icon = icon.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let sdk = PathBuf::from(
        env::var_os("ProgramFiles(x86)").expect("Windows SDK requires ProgramFiles(x86)"),
    )
    .join("Windows Kits/10/bin");
    let mut compilers: Vec<_> = fs::read_dir(sdk)
        .expect("install the Windows 10/11 SDK")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("10."))
        .map(|entry| entry.path().join("x64/rc.exe"))
        .filter(|path| path.is_file())
        .collect();
    compilers.sort();
    let compiler = compilers.last().expect("Windows SDK rc.exe is required");
    let status = Command::new(compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg(&source)
        .status()
        .expect("cannot run Windows resource compiler");
    assert!(status.success(), "Windows resource compilation failed");
    println!("cargo:rustc-link-arg={}", resource.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
