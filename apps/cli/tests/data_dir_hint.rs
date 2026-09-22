// Symlinks sind der Gegenstand dieser Datei, und der Zettel entsteht ohnehin
// nur auf macOS und Linux -- unter Windows liegt die Binaerdatei ohne
// verwalteten Ordner im PATH. Ohne diese Zeile scheitert dort schon das
// Uebersetzen des Testlaufs an `std::os::unix`.
#![cfg(unix)]

//! Der Hinweis der Desktop-App, geprueft auf dem ECHTEN Aufrufweg.
//!
//! Warum ein Integrationstest und kein Unit-Test: der Hinweis wird neben der
//! verwalteten Binaerdatei gesucht, und die findet die CLI ueber
//! `std::env::current_exe()`. Aufgerufen wird sie aber ueber den Symlink
//! ~/.local/bin/mitschnitt, und `current_exe` gibt auf macOS genau diesen
//! Symlink zurueck statt sein Ziel. Ein Unit-Test kann `current_exe` nicht
//! stellen und haette den Fall nie beruehrt -- er ist am 09.09.2026 auch
//! prompt durchgerutscht, bis der Handlauf am echten System scheiterte.
//!
//! Diese Tests starten deshalb das gebaute Binaer, einmal direkt und einmal
//! ueber einen Symlink, und lesen die Zeile, die `doctor` ueber die Herkunft
//! des Pfades ausgibt.

use std::path::Path;
use std::process::Command;

const OWN_FOLDER: &str = "media.zickert.mitschnitt";

// `home` ersetzt das echte Benutzerverzeichnis: `dirs::data_dir()` leitet sich
// davon ab, und ohne diese Umlenkung waere "this build's default folder" in
// jedem Lauf die ECHTE Datenbank -- der Test haette sie geoeffnet und
// seine Aussage haenge an der Maschine, auf der er laeuft.
fn doctor_in(home: &Path, invoked_as: &Path) -> String {
    let output = Command::new(invoked_as)
        // Beide Variablen leeren: sie stehen in der Aufloesung VOR dem
        // Hinweis, eine geerbte Belegung wuerde den Test blind machen.
        .env_remove("MITSCHNITT_DB_PATH")
        .env_remove("MITSCHNITT_BASE")
        .env("HOME", home)
        .arg("doctor")
        .output()
        .expect("doctor should run");
    String::from_utf8_lossy(&output.stdout).to_string()
}

// Die Zeile, die den aufgeloesten Pfad traegt. Auf den GANZEN Bericht zu
// pruefen ist zu grob: er nennt einen verworfenen Hinweis jetzt ausdruecklich,
// ein "der fremde Pfad kommt im Text nicht vor" wuerde also am eigenen
// Fortschritt scheitern.
fn database_line(report: &str) -> &str {
    report
        .lines()
        .find(|line| line.starts_with("Database: "))
        .expect("doctor should report a database line")
}

fn write_hint(managed_dir: &Path, base: &Path) {
    std::fs::write(
        managed_dir.join("data-dir"),
        format!("{}\n", base.display()),
    )
    .expect("hint should be writable");
}

// Der Fall, fuer den es den Hinweis gibt: die App liegt in einer anderen
// Bauart als die CLI, und die CLI wird ueber einen Symlink aufgerufen.
#[test]
fn a_hint_is_followed_when_the_cli_is_called_through_a_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let managed_dir = dir.path().join("managed");
    std::fs::create_dir_all(&managed_dir).unwrap();
    let binary = managed_dir.join("0.1.0");
    std::fs::copy(env!("CARGO_BIN_EXE_mitschnitt"), &binary).unwrap();

    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let link = bin_dir.join("mitschnitt");
    std::os::unix::fs::symlink(&binary, &link).unwrap();

    let base = dir.path().join("Application Support").join(OWN_FOLDER);
    write_hint(&managed_dir, &base);

    let report = doctor_in(dir.path(), &link);
    assert!(
        report.contains("Resolved from: hint from the desktop app"),
        "the hint was ignored when called through a symlink:\n{report}"
    );
    assert!(
        database_line(&report).contains(&base.join("app.db").display().to_string()),
        "resolved to the wrong database:\n{report}"
    );
}

// Gegenprobe: ohne Hinweis bleibt es beim alten Verhalten, damit ein Lauf aus
// dem Bauordner nicht plötzlich woanders hinsieht.
#[test]
fn without_a_hint_the_build_default_still_wins() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("mitschnitt");
    std::fs::copy(env!("CARGO_BIN_EXE_mitschnitt"), &binary).unwrap();

    let report = doctor_in(dir.path(), &binary);
    assert!(
        report.contains("Resolved from: this build's default folder"),
        "a missing hint changed the resolution:\n{report}"
    );
    // Ein FEHLENDER Hinweis ist kein verworfener. Sonst waere die Meldung
    // wertlos, die den einzigen Fall sichtbar macht, fuer den es die Pruefung
    // ueberhaupt gibt.
    assert!(
        !report.contains("Discarded hint:"),
        "a missing hint was reported as discarded:\n{report}"
    );
}

// Der Zahn auf dem echten Aufrufweg: ein Hinweis in eine fremde Installation
// darf die CLI nie dorthin schicken, auch nicht ueber den Symlink.
#[test]
fn a_hint_into_a_foreign_installation_is_ignored_on_the_real_path() {
    for folder in ["hyprnote", "anarlog", "com.hyprnote.stable"] {
        let dir = tempfile::tempdir().unwrap();
        let managed_dir = dir.path().join("managed");
        std::fs::create_dir_all(&managed_dir).unwrap();
        let binary = managed_dir.join("0.1.0");
        std::fs::copy(env!("CARGO_BIN_EXE_mitschnitt"), &binary).unwrap();

        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let link = bin_dir.join("mitschnitt");
        std::os::unix::fs::symlink(&binary, &link).unwrap();

        let foreign = dir.path().join("Application Support").join(folder);
        write_hint(&managed_dir, &foreign);

        let report = doctor_in(dir.path(), &link);
        assert!(
            !database_line(&report).contains(&foreign.display().to_string()),
            "followed a hint into the foreign folder {folder}:\n{report}"
        );
        assert!(
            report.contains("Resolved from: this build's default folder"),
            "a foreign hint was not discarded for {folder}:\n{report}"
        );
        assert!(
            report.contains("Discarded hint:"),
            "the discarded hint into {folder} was not reported:\n{report}"
        );
    }
}

// Der Weg, den die Namenspruefung allein offen liess: ein Symlink mit EIGENEM
// Namen, der auf eine fremde Installation zeigt. Auf dem echten Aufrufweg.
#[test]
fn a_disguised_symlink_is_ignored_on_the_real_path() {
    let dir = tempfile::tempdir().unwrap();
    let managed_dir = dir.path().join("managed");
    std::fs::create_dir_all(&managed_dir).unwrap();
    let binary = managed_dir.join("0.1.0");
    std::fs::copy(env!("CARGO_BIN_EXE_mitschnitt"), &binary).unwrap();

    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let link = bin_dir.join("mitschnitt");
    std::os::unix::fs::symlink(&binary, &link).unwrap();

    let foreign = dir.path().join("Application Support").join("hyprnote");
    std::fs::create_dir_all(&foreign).unwrap();
    let disguised = dir.path().join("Application Support").join("Mitschnitt");
    std::os::unix::fs::symlink(&foreign, &disguised).unwrap();
    write_hint(&managed_dir, &disguised);

    let report = doctor_in(dir.path(), &link);
    assert!(
        !database_line(&report).contains(&foreign.display().to_string()),
        "followed a disguised symlink into a foreign installation:\n{report}"
    );
    assert!(
        report.contains("Discarded hint:"),
        "the disguised symlink was not reported as discarded:\n{report}"
    );
}
