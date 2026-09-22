use super::*;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::os::unix::fs::PermissionsExt;

// The desktop app installs this command on every startup, so its name decides
// which file in ~/.local/bin gets written. Upstream mapped every unrecognised
// identifier to "anarlog", which is the command name of a productive anarlog
// install. No identifier may produce an upstream name again.
#[test]
fn every_identifier_maps_to_the_fork_command_name() {
    for identifier in [
        anlg_storage::global::APP_BUNDLE_ID,
        "com.hyprnote.stable",
        "com.hyprnote.Hyprnote",
        "com.hyprnote.staging",
        "com.hyprnote.dev",
        "so.anarlog.Anarlog",
        "unknown",
        "",
    ] {
        let name = command_name_from_identifier(identifier);

        assert_eq!(name, "mitschnitt");
        assert!(
            !name.starts_with("anarlog"),
            "identifier {identifier} produced the upstream command name {name}"
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn resolves_linux_x64_bundled_binary() {
    assert_eq!(
        bundled_binary_name(),
        Some("mitschnitt-cli-x86_64-unknown-linux-gnu")
    );
    assert_eq!(sidecar_binary_name(), "mitschnitt-cli");
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
#[test]
fn resolves_linux_arm64_bundled_binary() {
    assert_eq!(
        bundled_binary_name(),
        Some("mitschnitt-cli-aarch64-unknown-linux-gnu")
    );
    assert_eq!(sidecar_binary_name(), "mitschnitt-cli");
}

#[test]
fn finds_windows_path_entries_case_insensitively() {
    let expected = Path::new(r"C:\Users\Test\AppData\Local\Mitschnitt\bin");

    assert!(path_list_contains(
        r"C:\Windows;C:\USERS\TEST\APPDATA\LOCAL\MITSCHNITT\BIN\;C:\Tools",
        expected
    ));
    assert!(!path_list_contains(
        r"C:\Windows;C:\Users\Test\AppData\Local\Other\bin",
        expected
    ));
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn classifies_missing_install() {
    let dir = tempfile::tempdir().unwrap();
    let resource_path = dir.path().join("anarlog-cli");
    std::fs::write(&resource_path, "cli").unwrap();

    let state = classify_installation(&dir.path().join("anarlog"), &resource_path).unwrap();
    assert_eq!(state, EmbeddedCliState::Missing);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn classifies_installed_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let managed_path = dir.path().join("managed-anarlog-cli");
    std::fs::write(&managed_path, "cli").unwrap();
    std::fs::set_permissions(&managed_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let install_path = dir.path().join("anarlog");
    std::os::unix::fs::symlink(&managed_path, &install_path).unwrap();

    let state = classify_installation(&install_path, &managed_path).unwrap();
    assert_eq!(state, EmbeddedCliState::Installed);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn classifies_non_executable_managed_cli_as_missing() {
    let dir = tempfile::tempdir().unwrap();
    let managed_path = dir.path().join("managed-anarlog-cli");
    std::fs::write(&managed_path, "cli").unwrap();
    std::fs::set_permissions(&managed_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let install_path = dir.path().join("anarlog");
    std::os::unix::fs::symlink(&managed_path, &install_path).unwrap();

    assert_eq!(
        classify_installation(&install_path, &managed_path).unwrap(),
        EmbeddedCliState::Missing
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn classifies_stale_symlinks_as_missing() {
    let dir = tempfile::tempdir().unwrap();
    let managed_path = dir.path().join("anarlog-cli");
    let old_managed_path = dir.path().join("old-anarlog-cli");
    let install_path = dir.path().join("anarlog");
    std::fs::write(&managed_path, "new cli").unwrap();
    std::fs::write(&old_managed_path, "old cli").unwrap();
    std::os::unix::fs::symlink(&old_managed_path, &install_path).unwrap();

    assert_eq!(
        classify_installation(&install_path, &managed_path).unwrap(),
        EmbeddedCliState::Missing
    );

    std::fs::remove_file(old_managed_path).unwrap();
    assert_eq!(
        classify_installation(&install_path, &managed_path).unwrap(),
        EmbeddedCliState::Missing
    );
}

#[cfg(target_os = "macos")]
#[test]
fn classifies_legacy_app_resource_symlink_as_missing() {
    let dir = tempfile::tempdir().unwrap();
    let managed_path = dir.path().join("managed-anarlog-cli");
    let app_resource_path = dir
        .path()
        .join("Anarlog.app/Contents/Resources/anarlog-cli");
    let install_path = dir.path().join("anarlog");
    std::fs::create_dir_all(app_resource_path.parent().unwrap()).unwrap();
    std::fs::write(&app_resource_path, "cli").unwrap();
    std::os::unix::fs::symlink(&app_resource_path, &install_path).unwrap();

    assert_eq!(
        classify_installation(&install_path, &managed_path).unwrap(),
        EmbeddedCliState::Missing
    );
}

#[cfg(target_os = "macos")]
#[test]
fn classifies_legacy_app_executable_symlink_as_missing() {
    let dir = tempfile::tempdir().unwrap();
    let managed_path = dir.path().join(".anarlog-cli/anarlog/1.2.0");
    let app_executable_path = dir.path().join("Anarlog.app/Contents/MacOS/anarlog-cli");
    let install_path = dir.path().join("anarlog");
    std::fs::create_dir_all(app_executable_path.parent().unwrap()).unwrap();
    std::fs::write(&app_executable_path, "cli").unwrap();
    std::os::unix::fs::symlink(&app_executable_path, &install_path).unwrap();

    assert_eq!(
        classify_installation(&install_path, &managed_path).unwrap(),
        EmbeddedCliState::Missing
    );
}

#[cfg(target_os = "macos")]
#[test]
fn installer_replaces_legacy_app_executable_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let resource_path = dir.path().join("bundled-anarlog-cli");
    let managed_path = dir.path().join(".anarlog-cli/anarlog/1.2.0");
    let app_executable_path = dir.path().join("Anarlog.app/Contents/MacOS/anarlog-cli");
    let install_path = dir.path().join("anarlog");
    std::fs::write(&resource_path, "new cli").unwrap();
    std::fs::create_dir_all(app_executable_path.parent().unwrap()).unwrap();
    std::fs::write(&app_executable_path, "old cli").unwrap();
    std::os::unix::fs::symlink(&app_executable_path, &install_path).unwrap();

    install_managed_cli(&resource_path, &managed_path, &install_path).unwrap();

    assert_eq!(std::fs::read_link(&install_path).unwrap(), managed_path);
    assert_eq!(std::fs::read_to_string(&install_path).unwrap(), "new cli");
}

// Der konkrete Alt-Zustand auf dem Rechner des Prinzipals: der Fork bundelte
// sein Sidecar bis 09/2026 unter dem geerbten Namen, in der eigenen App. Wer
// nur den neuen Namen kennt, haelt so einen Verweis fuer fremd und meldet
// Conflict -- dann bleibt der Knopf fuer immer blockiert, obwohl der Verweis
// aus unserem eigenen Haus stammt.
#[cfg(target_os = "macos")]
#[test]
fn classifies_the_forks_own_legacy_sidecar_link_as_missing() {
    let dir = tempfile::tempdir().unwrap();
    let managed_path = dir.path().join(".mitschnitt-cli/mitschnitt/1.2.0");
    let app_executable_path = dir.path().join("Mitschnitt.app/Contents/MacOS/anarlog-cli");
    let install_path = dir.path().join("mitschnitt");
    std::fs::create_dir_all(app_executable_path.parent().unwrap()).unwrap();
    std::fs::write(&app_executable_path, "cli").unwrap();
    std::os::unix::fs::symlink(&app_executable_path, &install_path).unwrap();

    assert_eq!(
        classify_installation(&install_path, &managed_path).unwrap(),
        EmbeddedCliState::Missing
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn classifies_foreign_symlink_as_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let managed_path = dir.path().join(".anarlog-cli/anarlog/1.2.0");
    let install_path = dir.path().join("anarlog");
    std::os::unix::fs::symlink("/opt/homebrew/bin/anarlog", &install_path).unwrap();

    assert_eq!(
        classify_installation(&install_path, &managed_path).unwrap(),
        EmbeddedCliState::Conflict
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn installer_refuses_to_replace_foreign_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let resource_path = dir.path().join("bundled-anarlog-cli");
    let managed_path = dir.path().join(".anarlog-cli/anarlog/1.2.0");
    let install_path = dir.path().join("anarlog");
    let foreign_target = Path::new("/opt/homebrew/bin/anarlog");
    std::fs::write(&resource_path, "cli").unwrap();
    std::os::unix::fs::symlink(foreign_target, &install_path).unwrap();

    assert!(install_managed_cli(&resource_path, &managed_path, &install_path).is_err());
    assert_eq!(std::fs::read_link(&install_path).unwrap(), foreign_target);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn installed_cli_survives_bundled_resource_move() {
    let dir = tempfile::tempdir().unwrap();
    let resource_path = dir.path().join("Anarlog.app/Contents/MacOS/anarlog-cli");
    let install_path = dir.path().join("home/.local/bin/anarlog");
    let managed_path = managed_binary_path(&install_path, "anarlog", "1.2.0").unwrap();
    std::fs::create_dir_all(resource_path.parent().unwrap()).unwrap();
    std::fs::write(&resource_path, "cli").unwrap();
    std::fs::set_permissions(&resource_path, std::fs::Permissions::from_mode(0o644)).unwrap();

    install_managed_cli(&resource_path, &managed_path, &install_path).unwrap();
    std::fs::remove_dir_all(dir.path().join("Anarlog.app")).unwrap();

    assert_eq!(std::fs::read_to_string(&install_path).unwrap(), "cli");
    assert_ne!(
        std::fs::metadata(&install_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0
    );
    assert_eq!(
        classify_installation(&install_path, &managed_path).unwrap(),
        EmbeddedCliState::Installed
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn app_update_requires_installing_the_new_cli_version() {
    let dir = tempfile::tempdir().unwrap();
    let install_path = dir.path().join("home/.local/bin/anarlog");
    let old_resource_path = dir.path().join("old-cli");
    let new_resource_path = dir.path().join("new-cli");
    let old_managed_path = managed_binary_path(&install_path, "anarlog", "1.2.0").unwrap();
    let new_managed_path = managed_binary_path(&install_path, "anarlog", "1.3.0").unwrap();
    std::fs::write(&old_resource_path, "old cli").unwrap();
    std::fs::write(&new_resource_path, "new cli").unwrap();
    install_managed_cli(&old_resource_path, &old_managed_path, &install_path).unwrap();

    assert_eq!(
        classify_installation(&install_path, &new_managed_path).unwrap(),
        EmbeddedCliState::Missing
    );

    install_managed_cli(&new_resource_path, &new_managed_path, &install_path).unwrap();
    assert_eq!(std::fs::read_to_string(&install_path).unwrap(), "new cli");
    assert_eq!(
        classify_installation(&install_path, &new_managed_path).unwrap(),
        EmbeddedCliState::Installed
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn classifies_regular_file_as_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let managed_path = dir.path().join("anarlog-cli");
    let install_path = dir.path().join("anarlog");
    std::fs::write(&managed_path, "cli").unwrap();
    std::fs::write(&install_path, "other").unwrap();

    let state = classify_installation(&install_path, &managed_path).unwrap();
    assert_eq!(state, EmbeddedCliState::Conflict);
}

// Die Erneuerung nach Inhalt hatte beim Bau keinen Test -- gefunden im
// Cross-Vendor-Audit vom 09.09.2026. Sie entscheidet, ob eine frisch gebaute
// Kommandozeile den Nutzer ueberhaupt erreicht, und war vorher an die
// Versionsnummer gekoppelt, die sich beim taeglichen Bau nie bewegt.
#[test]
fn a_bundled_cli_of_a_different_size_counts_as_changed() {
    let dir = tempfile::tempdir().unwrap();
    let resource = dir.path().join("bundled");
    let managed = dir.path().join("installed");
    std::fs::write(&resource, "zwei Befehle mehr").unwrap();
    std::fs::write(&managed, "alt").unwrap();

    assert!(differs_in_size(&resource, &managed));
}

// Der Fall, den die Groesse allein durchliess: gleich gross, anderer Inhalt.
// Entschieden wird dann ueber die Aenderungszeit.
#[test]
fn an_equally_sized_but_newer_bundled_cli_counts_as_changed() {
    let dir = tempfile::tempdir().unwrap();
    let managed = dir.path().join("installed");
    std::fs::write(&managed, "aaaa").unwrap();

    // Abstand erzwingen, damit der Vergleich nicht an der Aufloesung der
    // Zeitstempel scheitert.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let resource = dir.path().join("bundled");
    std::fs::write(&resource, "bbbb").unwrap();

    assert!(differs_in_size(&resource, &managed));
}

// Gegenprobe: derselbe Stand darf keine Neuinstallation ausloesen, sonst
// schreibt die App bei jedem Start 7,6 MB neu.
#[test]
fn an_unchanged_bundled_cli_counts_as_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let resource = dir.path().join("bundled");
    let managed = dir.path().join("installed");
    std::fs::write(&resource, "gleich").unwrap();
    std::fs::copy(&resource, &managed).unwrap();
    // `copy` uebernimmt den Inhalt, nicht die Zeit -- das Ziel ist also juenger
    // oder gleich alt. Genau der Zustand nach einer echten Installation.

    assert!(!differs_in_size(&resource, &managed));
}

// Nur ein sicheres Ja ersetzt: eine fehlende Seite darf keine Endlosschleife
// aus Neuinstallieren ausloesen.
#[test]
fn an_unreadable_side_never_triggers_a_reinstall() {
    let dir = tempfile::tempdir().unwrap();
    let managed = dir.path().join("installed");
    std::fs::write(&managed, "da").unwrap();
    let missing = dir.path().join("gibt-es-nicht");

    assert!(!differs_in_size(&missing, &managed));
    assert!(!differs_in_size(&managed, &missing));
}
