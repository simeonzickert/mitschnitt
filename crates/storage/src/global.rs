use std::path::{Path, PathBuf};

pub const VAULT_CONFIG_FILENAME: &str = "global.json";
const STAGING_BUNDLE_ID: &str = "media.zickert.mitschnitt.staging";
// tauri.conf.stable.json liefert diese Kennung aus. Sie fehlte hier, womit ein
// Debug-Bau des stable-Kanals seinen eigenen Datenordner nicht als eigenen
// erkannt haette.
const STABLE_BUNDLE_ID: &str = "media.zickert.mitschnitt.stable";
const RELEASE_APP_FOLDER: &str = "Mitschnitt";

// The fork's single identity, mirroring `identifier` in
// apps/desktop/src-tauri/tauri.conf.json. Binaries that are not the Tauri app
// (the CLI) have no config to read it from and must not re-derive it from
// their own file name -- that is how the CLI ended up defaulting to upstream's
// bundle id and writing into a foreign data directory.
pub const APP_BUNDLE_ID: &str = "media.zickert.mitschnitt";

pub fn compute_vault_config_path(base: &Path) -> PathBuf {
    base.join(VAULT_CONFIG_FILENAME)
}

pub fn compute_default_base(bundle_id: &str) -> Option<PathBuf> {
    let data_dir = dirs::data_dir()?;
    let app_folder = resolve_app_folder(bundle_id, cfg!(debug_assertions));
    Some(data_dir.join(app_folder))
}

// Mitschnitt is a fork and must never adopt the data directory of an existing
// anarlog/Hyprnote install. Upstream resolved the release folder by probing the
// data dir and falling back to the legacy `hyprnote` folder when it held data;
// on a machine with a productive anarlog install that fallback would have
// pointed this build at the user's live database (and the launch lock, the
// settings vault, and the auth migration in plugins/auth, which *renames*
// auth.json out of the legacy folder). The probe is gone: the folder is now
// derived from the fork's own identity only.
pub fn resolve_app_folder(bundle_id: &str, is_debug: bool) -> &str {
    if is_debug || bundle_id == STAGING_BUNDLE_ID {
        bundle_id
    } else {
        RELEASE_APP_FOLDER
    }
}

// Der Datenordner haengt an der Bauart: ein Debug-Bau schreibt in den Ordner
// der Bundle-Kennung, ein Release-Bau in "Mitschnitt". Das ist Absicht -- ein
// Entwicklungsbau darf die produktiven Daten nicht anfassen. Es bricht aber,
// sobald zwei Binaerdateien DERSELBEN Installation in verschiedenen Bauarten
// vorliegen: die Desktop-App laeuft im Alltag als Debug-Bau und schreibt nach
// media.zickert.mitschnitt, die von ihr installierte CLI ist ein Release-Bau
// und suchte in Mitschnitt. Beide folgten derselben Regel und landeten in
// verschiedenen Ordnern; die CLI meldete "database file does not exist",
// waehrend die Datenbank danebenlag (gemessen 2026-09-09).
//
// Die App kennt ihren Datenordner exakt. Sie legt ihn beim Installieren der
// CLI als Hinweisdatei neben die verwaltete Binaerdatei; die CLI liest ihn,
// bevor sie auf ihre eigene Bauart zurueckfaellt. Damit ist die Bauart egal
// und es entsteht KEINE Ratekette ueber fremde Ordner.
pub const DATA_DIR_HINT_FILENAME: &str = "data-dir";

// Der Zahn gegen die Rueckkehr der Kandidatenkette: ein Hinweis wird nur
// befolgt, wenn er auf einen Ordner DIESES Forks zeigt. Ein Hinweis auf
// hyprnote/anarlog/com.hyprnote.* -- egal ob durch einen Fehler geschrieben
// oder von Hand hineingelegt -- wird verworfen, nicht befolgt.
pub fn is_own_app_folder(folder_name: &str) -> bool {
    folder_name == RELEASE_APP_FOLDER
        || folder_name == APP_BUNDLE_ID
        || folder_name == STAGING_BUNDLE_ID
        || folder_name == STABLE_BUNDLE_ID
}

pub fn compute_data_dir_hint_path(managed_dir: &Path) -> PathBuf {
    managed_dir.join(DATA_DIR_HINT_FILENAME)
}

// Was beim Lesen des Hinweises herauskam. Ein VERWORFENER Hinweis ist etwas
// anderes als gar keiner: keiner ist der Normalfall (CLI aus dem Bauordner),
// verworfen ist das Ereignis, fuer das die Pruefung ueberhaupt existiert. Als
// blosses `None` waere genau dieses Ereignis stumm -- derselbe Fehler, den
// `doctor` eine Ebene tiefer schon einmal gemacht hat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataDirHint {
    Absent,
    Rejected(String),
    Found(PathBuf),
}

// Liest den Hinweis und gibt ihn nur zurueck, wenn er auf einen eigenen Ordner
// zeigt. Ein fehlender Hinweis ist der Normalfall fuer eine CLI, die direkt aus
// dem Bauordner laeuft.
pub fn read_data_dir_hint(managed_dir: &Path) -> DataDirHint {
    let path = compute_data_dir_hint_path(managed_dir);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        // Nicht vorhanden und unlesbar sind hier bewusst dasselbe: beides
        // heisst "kein brauchbarer Hinweis", und ein Rechtefehler auf dieser
        // Datei ist kein Zustand, den ein Nutzer auseinanderhalten muesste.
        return DataDirHint::Absent;
    };

    // Nur die erste Zeile. Aus zwei Zeilen sonst EIN Pfad mit eingebettetem
    // Zeilenumbruch zu bauen ist raten, nicht lesen.
    let Some(first_line) = raw.lines().next().map(str::trim) else {
        return DataDirHint::Absent;
    };
    if first_line.is_empty() {
        return DataDirHint::Absent;
    }

    let base = PathBuf::from(first_line);
    if !base.is_absolute() {
        return DataDirHint::Rejected(format!("hint is not an absolute path: {first_line}"));
    }

    // Aufloesen VOR der Namenspruefung. Sonst prueft die Regel nur den Namen
    // der letzten Komponente, und ein Symlink namens "Mitschnitt", der auf den
    // Ordner einer fremden anarlog-Installation zeigt, kaeme durch -- der
    // Kommentar oben verspricht aber das Ziel, nicht den Namen.
    //
    // Existiert der Ordner noch nicht (frische Installation), gibt es nichts
    // aufzuloesen und damit auch keinen Symlink, der etwas verstecken koennte;
    // dann traegt die Namenspruefung allein.
    let resolved = base.canonicalize().unwrap_or(base);

    let Some(folder_name) = resolved.file_name().and_then(|name| name.to_str()) else {
        return DataDirHint::Rejected(format!("hint has no folder name: {}", resolved.display()));
    };
    if !is_own_app_folder(folder_name) {
        return DataDirHint::Rejected(format!(
            "hint points outside this fork: {}",
            resolved.display()
        ));
    }

    DataDirHint::Found(resolved)
}

pub fn write_data_dir_hint(managed_dir: &Path, base: &Path) -> std::io::Result<()> {
    // Ein Hinweis, der auf einen fremden Ordner zeigt, wird gar nicht erst
    // geschrieben. Sonst waere der Lesepfad die einzige Verteidigung.
    let resolved = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    let folder_name = resolved
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !resolved.is_absolute() || !is_own_app_folder(folder_name) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "refusing to write a data-dir hint outside this fork: {}",
                base.display()
            ),
        ));
    }

    std::fs::create_dir_all(managed_dir)?;

    // Ueber eine temporaere Datei und `rename`, wie die Installation direkt
    // daneben es fuer Binaerdatei und Symlink auch macht: die CLI kann jederzeit
    // mitlesen, und ein abgebrochener Schreibvorgang darf keinen halben Pfad
    // hinterlassen.
    let target = compute_data_dir_hint_path(managed_dir);
    let temp = managed_dir.join(format!(
        ".{DATA_DIR_HINT_FILENAME}.tmp-{}",
        std::process::id()
    ));

    // `create_new` statt eines einfachen Schreibens: laege dort schon etwas,
    // koennte es ein Symlink auf eine fremde Datenbank sein, und ein Schreiben
    // wuerde ihm folgen und die Zieldatei abschneiden. Der Weg ueber `rename`
    // schuetzt den Zettel selbst -- ein Umbenennen ersetzt den
    // Verzeichniseintrag und folgt keinem Symlink --, aber die Zwischendatei
    // braucht denselben Schutz. Ein Rest aus einem abgestuerzten Lauf wird
    // vorher weggeraeumt; das ist derselbe Griff, den die Installation der
    // Binaerdatei daneben auch macht.
    let _ = std::fs::remove_file(&temp);
    {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(format!("{}\n", resolved.display()).as_bytes())?;
    }
    match std::fs::rename(&temp, &target) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_builds_use_the_fork_folder() {
        for bundle_id in [
            "media.zickert.mitschnitt",
            "com.hyprnote.stable",
            "com.hyprnote.Hyprnote",
        ] {
            assert_eq!(resolve_app_folder(bundle_id, false), RELEASE_APP_FOLDER);
        }
    }

    // Regression guard for the removed legacy probe. A productive anarlog or
    // Hyprnote install keeps its data in these folders, so no release build of
    // this fork may ever resolve to one of them.
    //
    // This asserts through resolve_app_folder, not against RELEASE_APP_FOLDER:
    // an upstream merge that restores the probe would reintroduce a *branch*
    // that returns "hyprnote" while leaving the constant at "Mitschnitt". A
    // test on the constant alone would stay green through exactly the
    // regression it is meant to catch.
    #[test]
    fn no_release_build_ever_resolves_to_an_upstream_folder() {
        for bundle_id in [
            APP_BUNDLE_ID,
            "com.hyprnote.stable",
            "com.hyprnote.Hyprnote",
            "com.anarlog.stable",
            "unknown.bundle.id",
        ] {
            let folder = resolve_app_folder(bundle_id, false).to_lowercase();
            for upstream in ["hyprnote", "anarlog", "char"] {
                assert_ne!(
                    folder, upstream,
                    "release build for {bundle_id} resolved to the upstream folder {upstream}"
                );
            }
        }
    }

    // Gegen STAGING_BUNDLE_ID zu pruefen sagt nichts darueber aus, ob die
    // Konstante den RICHTIGEN Wert hat -- der Test war gruen, waehrend sie auf
    // die Kennung des Originals zeigte und ein Staging-Release deshalb in den
    // Produktiv-Ordner schrieb. Deshalb steht hier der Literalwert.
    #[test]
    fn staging_gets_its_own_folder_and_never_the_release_folder() {
        assert_eq!(
            resolve_app_folder("media.zickert.mitschnitt.staging", false),
            "media.zickert.mitschnitt.staging"
        );
        assert_ne!(
            resolve_app_folder("media.zickert.mitschnitt.staging", false),
            RELEASE_APP_FOLDER,
            "Staging teilt sich den Datenordner mit dem Produktivbau"
        );
    }

    #[test]
    fn resolve_app_folder_returns_bundle_id_in_debug_builds() {
        assert_eq!(
            resolve_app_folder("media.zickert.mitschnitt", true),
            "media.zickert.mitschnitt"
        );
    }

    // Der Hinweis traegt genau die beiden Ordner, die eine Bauart dieses Forks
    // erzeugen kann -- Release und Debug. Beide muessen durchkommen, sonst
    // loest der Hinweis den Bauart-Bruch nicht, wegen dem es ihn gibt.
    #[test]
    fn a_hint_into_either_own_folder_is_followed() {
        let dir = tempfile::tempdir().unwrap();
        for folder in [
            RELEASE_APP_FOLDER,
            APP_BUNDLE_ID,
            STAGING_BUNDLE_ID,
            STABLE_BUNDLE_ID,
        ] {
            let base = dir.path().join("Application Support").join(folder);
            write_data_dir_hint(dir.path(), &base).unwrap();
            assert_eq!(read_data_dir_hint(dir.path()), DataDirHint::Found(base));
        }
    }

    // Der Befund, der die Namenspruefung als das entlarvt hat, was sie war:
    // eine Pruefung des NAMENS, nicht des ZIELS. Ein Verzeichnis-Symlink, der
    // "Mitschnitt" heisst und auf die Daten einer fremden Installation zeigt,
    // kam durch. Beide Seiten muessen ihn abweisen.
    #[test]
    fn a_symlink_with_an_own_name_into_a_foreign_folder_is_refused_and_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let foreign = dir.path().join("Application Support").join("hyprnote");
        std::fs::create_dir_all(&foreign).unwrap();

        let disguised = dir.path().join("Application Support").join("Mitschnitt");
        std::os::unix::fs::symlink(&foreign, &disguised).unwrap();

        assert!(
            write_data_dir_hint(dir.path(), &disguised).is_err(),
            "wrote a hint through a symlink disguised with an own name"
        );

        std::fs::write(
            compute_data_dir_hint_path(dir.path()),
            format!("{}\n", disguised.display()),
        )
        .unwrap();
        match read_data_dir_hint(dir.path()) {
            DataDirHint::Rejected(_) => {}
            other => panic!("followed a disguised symlink into a foreign folder: {other:?}"),
        }
    }

    // Ein verworfener Hinweis muss von "gar kein Hinweis" unterscheidbar sein,
    // sonst ist der einzige Fall, fuer den es die Pruefung gibt, unsichtbar.
    #[test]
    fn a_rejected_hint_is_reported_as_rejected_not_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let foreign = dir.path().join("Application Support").join("anarlog");
        std::fs::write(
            compute_data_dir_hint_path(dir.path()),
            format!("{}\n", foreign.display()),
        )
        .unwrap();

        match read_data_dir_hint(dir.path()) {
            DataDirHint::Rejected(reason) => assert!(
                reason.contains("outside this fork"),
                "reason does not say what happened: {reason}"
            ),
            other => panic!("a foreign hint was not reported as rejected: {other:?}"),
        }
    }

    // Aus zwei Zeilen einen Pfad mit eingebettetem Zeilenumbruch zu bauen waere
    // raten. Gelesen wird die erste Zeile.
    #[test]
    fn only_the_first_line_of_the_hint_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("Application Support").join(APP_BUNDLE_ID);
        std::fs::write(
            compute_data_dir_hint_path(dir.path()),
            format!("{}\nirgendwas dahinter\n", base.display()),
        )
        .unwrap();

        assert_eq!(read_data_dir_hint(dir.path()), DataDirHint::Found(base));
    }

    // Der Zahn. Ein Hinweis auf die Daten einer fremden Installation wird auf
    // BEIDEN Seiten abgewiesen: er laesst sich nicht schreiben, und ein von
    // Hand hineingelegter wird beim Lesen verworfen.
    #[test]
    fn a_hint_into_a_foreign_installation_is_refused_and_ignored() {
        for folder in ["hyprnote", "anarlog", "com.hyprnote.stable"] {
            let dir = tempfile::tempdir().unwrap();
            let foreign = dir.path().join("Application Support").join(folder);

            assert!(
                write_data_dir_hint(dir.path(), &foreign).is_err(),
                "wrote a hint into the foreign folder {folder}"
            );

            std::fs::write(
                compute_data_dir_hint_path(dir.path()),
                format!("{}\n", foreign.display()),
            )
            .unwrap();
            match read_data_dir_hint(dir.path()) {
                DataDirHint::Rejected(_) => {}
                other => panic!(
                    "followed a hand-written hint into the foreign folder {folder}: {other:?}"
                ),
            }
        }
    }

    #[test]
    fn a_missing_empty_or_relative_hint_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            read_data_dir_hint(dir.path()),
            DataDirHint::Absent,
            "missing hint"
        );

        for content in ["", "   \n"] {
            std::fs::write(compute_data_dir_hint_path(dir.path()), content).unwrap();
            assert_eq!(
                read_data_dir_hint(dir.path()),
                DataDirHint::Absent,
                "an empty hint should read as absent, not as rejected: {content:?}"
            );
        }

        for content in ["Mitschnitt", "./Mitschnitt"] {
            std::fs::write(compute_data_dir_hint_path(dir.path()), content).unwrap();
            match read_data_dir_hint(dir.path()) {
                DataDirHint::Rejected(_) => {}
                other => panic!("followed the relative hint {content:?}: {other:?}"),
            }
        }
    }

    // Ein zweiter Lauf der App darf den Hinweis nicht verdoppeln oder an einen
    // alten Wert anhaengen, sonst zeigt er nach einem Bauartwechsel weiter auf
    // den alten Ordner.
    #[test]
    fn writing_the_hint_twice_replaces_it() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("one").join(RELEASE_APP_FOLDER);
        let second = dir.path().join("two").join(APP_BUNDLE_ID);

        write_data_dir_hint(dir.path(), &first).unwrap();
        write_data_dir_hint(dir.path(), &second).unwrap();

        assert_eq!(read_data_dir_hint(dir.path()), DataDirHint::Found(second));

        // Kein temporaerer Rest neben dem Zettel: das Schreiben laeuft ueber
        // eine Zwischendatei und `rename`, und die muss danach weg sein.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with('.') && name.contains(DATA_DIR_HINT_FILENAME))
            .collect();
        assert!(leftovers.is_empty(), "temporary files left behind: {leftovers:?}");
    }
}
