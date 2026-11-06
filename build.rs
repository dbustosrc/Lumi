use std::{
    collections::BTreeSet,
    env,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn embed_third_party_notices(root: &Path, out: &Path) {
    let notices = root.join("third-party-licenses");
    println!("cargo:rerun-if-changed={}", notices.display());
    let mut directories: BTreeSet<_> = fs::read_dir(&notices)
        .expect("read the third-party notices directory")
        .map(|entry| entry.expect("read third-party directory entry").path())
        .filter(|path| path.is_dir())
        .collect();
    let index = fs::read_to_string(notices.join("README.md")).expect("read third-party index");
    let mut source = String::from("pub(super) static COMPONENTS: &[Component] = &[\n");
    let mut count = 0;
    for row in index.lines().filter(|line| line.starts_with('|')).skip(2) {
        let columns: Vec<_> = row.trim_matches('|').split('|').map(str::trim).collect();
        let [name, version, license, _] = columns.as_slice() else {
            panic!("invalid row in third-party index: {row}");
        };
        let directory = notices.join(format!("{name}-{version}"));
        assert!(
            directories.remove(&directory),
            "missing or duplicate notice directory: {}",
            directory.display()
        );
        let mut text = String::new();
        append_notice_files(&directory, &directory, &mut text);
        assert!(
            !text.is_empty(),
            "empty notice directory: {}",
            directory.display()
        );
        let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
        writeln!(source, "    Component {{ name: {name:?}, version: {version:?}, license: {license:?}, text: {text:?} }},").unwrap();
        count += 1;
    }
    assert!(
        count > 0 && directories.is_empty(),
        "third-party index omits notice directories: {directories:?}"
    );
    source.push_str("];\n");
    fs::write(out.join("third_party.rs"), source).expect("embed third-party notices");
}

fn append_notice_files(root: &Path, directory: &Path, text: &mut String) {
    let mut entries: Vec<_> = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("read notice file entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            append_notice_files(root, &path, text);
        } else {
            let content = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            writeln!(
                text,
                "===== {} =====\n\n{content}\n",
                path.strip_prefix(root).unwrap().display()
            )
            .unwrap();
        }
    }
}

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
    embed_third_party_notices(&root, &out);
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
