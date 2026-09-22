use crate::make_specta_builder;

#[test]
fn export_types() {
    const OUTPUT_FILE: &str = "./js/bindings.gen.ts";

    make_specta_builder::<tauri::Wry>()
        .export(
            specta_typescript::Typescript::default()
                .formatter(specta_typescript::formatter::prettier)
                .bigint(specta_typescript::BigIntExportBehavior::Number),
            OUTPUT_FILE,
        )
        .unwrap();

    let content = std::fs::read_to_string(OUTPUT_FILE).unwrap();
    std::fs::write(OUTPUT_FILE, format!("// @ts-nocheck\n{content}")).unwrap();
}

#[test]
fn default_permissions_include_legacy_migration_workflow() {
    let permissions = include_str!("../../permissions/default.toml");

    for permission in [
        "allow-get-legacy-cleanup-status",
        "allow-get-legacy-import-report",
        "allow-cleanup-legacy-files",
        "allow-run-legacy-import",
    ] {
        assert!(permissions.contains(permission), "missing {permission}");
    }
}

#[test]
fn default_permissions_include_session_ingest() {
    let permissions = include_str!("../../permissions/default.toml");

    assert!(permissions.contains("allow-apply-session-ingest"));
}

#[test]
fn default_permissions_include_startup_ready() {
    let permissions = include_str!("../../permissions/default.toml");

    assert!(permissions.contains("allow-get-startup-status"));
    assert!(permissions.contains("allow-wait-until-ready"));
}

#[test]
fn default_permissions_carry_no_cloud_commands() {
    let permissions = include_str!("../../permissions/default.toml");

    assert!(!permissions.contains("cloudsync"));
    assert!(!permissions.contains("e2ee"));
}
