use std::path::PathBuf;

fn source(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()))
}

#[test]
fn cross_platform_unix_surfaces_have_fail_closed_stubs() {
    let contribution = source("contribution_loading.rs");
    for method in [
        "open_existing",
        "open_or_create",
        "write_single_file_directory",
        "write_files_directory",
        "import_directory",
        "import_extension_directory",
        "import_extension_directory_with_state",
        "replace_from_snapshot",
        "remove_directory",
        "remove_entry",
        "open_directory",
        "entry_names",
        "write_file",
        "remove_file",
        "write_file_in_directory",
        "read_file_in_directory",
        "write_file_in_existing_directory",
    ] {
        let signature = format!("pub(crate) fn {method}");
        let has_stub = contribution.match_indices(&signature).any(|(offset, _)| {
            let prefix = &contribution[offset.saturating_sub(160)..offset];
            prefix.contains("#[cfg(not(unix))]")
        });
        assert!(has_stub, "missing non-Unix stub for {method}");
    }
    assert!(contribution.contains("guarded contribution mutation requires Unix"));

    let proxy = source("code_act_proxy.rs");
    assert!(proxy.contains("code-act proxy requires Unix domain sockets"));
    assert!(proxy.contains("#[cfg(not(unix))]\n    pub fn new(_cwd: PathBuf) -> Result<Self>"));
    let ipc = source("ipc/mod.rs");
    assert!(ipc.contains("native IPC requires Unix domain sockets"));
    let serve = source("tools/serve.rs");
    assert!(serve.contains("background service management requires Unix process signaling"));
}
