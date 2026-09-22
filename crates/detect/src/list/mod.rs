#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn list_installed_apps() -> Vec<InstalledApp> {
    Vec::new()
}

// Diese drei Listen beantworten allein die Frage "benutze ICH gerade das
// Mikrofon?", damit die App sich nicht selbst als fremden Mithoerer meldet;
// geschrieben wird hier nichts. Sie fuehren NUR den Fork. Bis zum 02.09.2026
// standen hier auch die Kennungen und Namen des Originals (anarlog, hyprnote,
// char) -- mit der Folge, dass eine parallel laufende echte Installation des
// Originals am Mikrofon verschwiegen statt gemeldet wurde. Entscheid
// (ZICK-270): das Original ist eine fremde Meeting-App wie Zoom und wird
// angezeigt. Der Schutz vor fremden Datenordnern liegt woanders und kennt die
// Altnamen weiterhin (`apps/cli/src/db.rs`, `embedded_cli.rs`).
//
// AUSGELIEFERT wird KEINE dieser Kennungen nackt ausser der dev-Fassung: die
// tauri.conf-Dateien vergeben `media.zickert.mitschnitt` (dev),
// `.stable`, `.staging`, `.flatpak` und `.desktop`. Bis zum 01.09.2026 stand
// hier nur die nackte Kennung -- ein Stable- oder Staging-Bau hat sich also
// selbst als fremden Mithoerer am Mikrofon gemeldet. Die Schwesterliste in
// `plugins/detect/src/policy.rs` kannte alle fuenf; diese hier nicht. Wer eine
// neue tauri.conf-Variante anlegt, traegt sie an BEIDEN Stellen nach.
const SELF_BUNDLE_IDS: &[&str] = &[
    "media.zickert.mitschnitt",
    "media.zickert.mitschnitt.stable",
    "media.zickert.mitschnitt.staging",
    "media.zickert.mitschnitt.flatpak",
    "media.zickert.mitschnitt.desktop",
];

const SELF_APP_NAMES: &[&str] = &[
    "mitschnitt",
    // productName der tauri.conf.staging.json
    "mitschnitt staging",
    // Windows leitet den Anzeigenamen aus mainBinaryName ab
    // (tauri.conf.staging.json:4); ohne diesen Eintrag listete sich ein
    // Windows-Staging-Bau selbst als fremden Mithoerer (E4 a, Opus).
    "mitschnitt-staging",
];

const SELF_APP_PATH_SEGMENTS: &[&str] = &["/mitschnitt.app/", "/mitschnitt staging.app/"];

fn is_self_app(app: &InstalledApp) -> bool {
    let id = app.id.to_lowercase();
    let name = app.name.to_lowercase();

    SELF_BUNDLE_IDS.contains(&id.as_str())
        || SELF_APP_NAMES.contains(&name.as_str())
        || SELF_APP_PATH_SEGMENTS
            .iter()
            .any(|segment| id.contains(segment))
}

pub fn list_mic_using_apps() -> Result<Vec<InstalledApp>, crate::Error> {
    let apps = {
        #[cfg(target_os = "macos")]
        {
            macos::list_mic_using_apps()?
        }
        #[cfg(target_os = "linux")]
        {
            linux::list_mic_using_apps()?
        }
        #[cfg(target_os = "windows")]
        {
            windows::list_mic_using_apps()?
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            Vec::<InstalledApp>::new()
        }
    };

    Ok(without_self(apps))
}

fn without_self(apps: Vec<InstalledApp>) -> Vec<InstalledApp> {
    apps.into_iter().filter(|app| !is_self_app(app)).collect()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str) -> InstalledApp {
        InstalledApp {
            id: id.to_string(),
            name: name.to_string(),
        }
    }

    // Regressions-Wache fuer den Fork selbst: die eigene Kennung MUSS erkannt
    // werden, sonst meldet die App sich als fremder Mithoerer am Mikrofon.
    #[test]
    fn test_is_self_app_matches_the_fork_itself() {
        // Jede der drei Listen wird EINZELN geprueft: der Name ist bewusst
        // nichtssagend, wo die Kennung treffen soll, und die Kennung
        // nichtssagend, wo der Name treffen soll. Sonst deckt eine Liste den
        // Ausfall der anderen zu und der Test kann nicht mehr rot werden.
        assert!(
            is_self_app(&app("media.zickert.mitschnitt", "Unknown")),
            "eigene Bundle-Kennung fehlt in SELF_BUNDLE_IDS"
        );
        assert!(
            is_self_app(&app("pid:1", "Mitschnitt")),
            "eigener Anzeigename fehlt in SELF_APP_NAMES"
        );
        assert!(
            is_self_app(&app(
                "/Applications/Mitschnitt.app/Contents/MacOS/mitschnitt",
                "Unknown",
            )),
            "eigener Pfad fehlt in SELF_APP_PATH_SEGMENTS"
        );
    }

    // Entscheid 02.09.2026 (ZICK-270): das Original ist eine fremde
    // Meeting-App wie Zoom und wird am Mikrofon gemeldet, nicht verschwiegen.
    // Jede Achse einzeln (Kennung, Name, Pfad), damit keine Liste den Rest
    // einer anderen zudeckt.
    #[test]
    fn the_original_product_is_not_mistaken_for_the_fork() {
        for (id, name) in [
            ("com.hyprnote.stable", "Unknown"),
            ("com.anarlog.stable", "Unknown"),
            ("com.hyprnote.nightly", "Unknown"),
            ("pid:42", "Anarlog"),
            ("pid:43", "Hyprnote Staging"),
            ("pid:44", "Char Nightly"),
            (
                "/Applications/Anarlog.app/Contents/MacOS/anarlog",
                "Unknown",
            ),
            (
                "/Applications/Hyprnote Nightly.app/Contents/MacOS/Hyprnote Nightly",
                "Unknown",
            ),
        ] {
            assert!(
                !is_self_app(&app(id, name)),
                "{id} / {name} gilt noch als eigene App und wuerde verschwiegen"
            );
        }
    }

    // Auf der Ebene, die der Aufrufer sieht: das Original bleibt in der
    // Liste, der Fork selbst faellt heraus.
    #[test]
    fn the_original_product_stays_in_the_mic_list_while_the_fork_drops_out() {
        let listed = without_self(vec![
            app("com.hyprnote.stable", "Anarlog"),
            app("media.zickert.mitschnitt.stable", "Mitschnitt"),
            app("us.zoom.xos", "zoom.us"),
        ]);

        let ids: Vec<&str> = listed.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["com.hyprnote.stable", "us.zoom.xos"]);
    }

    // Der Test darueber prueft die NACKTE Kennung -- und die traegt nur der
    // dev-Bau. Genau daran ging die Luecke vom 01.09.2026 vorbei: Stable,
    // Staging und Flatpak melden sich unter einem Suffix, das in
    // SELF_BUNDLE_IDS fehlte, und der Test blieb trotzdem gruen.
    //
    // Diese Wache liest deshalb die AUSGELIEFERTEN Kennungen aus den
    // tauri.conf-Dateien selbst, statt sie hier zu wiederholen: eine neue
    // Bau-Variante macht den Test rot, ohne dass jemand daran denken muss.
    // Bewusst ohne serde_json (die Kiste hat es nicht) -- die Zeile
    // `"identifier": "..."` reicht.
    #[test]
    fn test_is_self_app_matches_every_shipped_bundle_id() {
        let tauri_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src-tauri");

        let mut identifiers: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&tauri_dir).expect("src-tauri lesbar") {
            let path = entry.expect("Verzeichniseintrag").path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !(name.starts_with("tauri.conf") && name.ends_with(".json")) {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("conf lesbar");
            for line in text.lines() {
                let line = line.trim();
                let Some(rest) = line.strip_prefix("\"identifier\":") else {
                    continue;
                };
                let value = rest.trim().trim_end_matches(',').trim_matches('"');
                if !value.is_empty() {
                    identifiers.push(value.to_string());
                }
            }
        }

        assert!(
            identifiers.len() >= 4,
            "keine tauri.conf-Kennungen gefunden ({identifiers:?}) -- \
             der Test prueft dann nichts mehr"
        );

        for id in identifiers {
            assert!(
                is_self_app(&app(&id, "Unknown")),
                "ausgelieferte Kennung {id} fehlt in SELF_BUNDLE_IDS -- \
                 dieser Bau meldet sich selbst als fremden Mithoerer"
            );
        }
    }

    #[test]
    fn test_is_self_app_matches_the_staging_build() {
        assert!(is_self_app(&app("pid:42", "Mitschnitt Staging")));
        // Windows: Name aus mainBinaryName (E4 a).
        assert!(is_self_app(&app("pid:43", "mitschnitt-staging")));
        assert!(is_self_app(&app(
            "/Applications/Mitschnitt Staging.app/Contents/MacOS/mitschnitt-staging",
            "Unknown",
        )));
    }

    #[test]
    fn test_is_self_app_does_not_match_unrelated_char_apps() {
        assert!(!is_self_app(&app(
            "com.adobe.character-animator",
            "Character Animator"
        )));
        assert!(!is_self_app(&app(
            "/Applications/Chart.app/Contents/MacOS/Chart",
            "Chart"
        )));
    }
}
