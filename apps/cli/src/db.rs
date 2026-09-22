use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anlg_storage::global::{APP_BUNDLE_ID, DataDirHint, is_own_app_folder, read_data_dir_hint};

use crate::{Args, Error, Result};

pub async fn open(args: &Args) -> Result<anlg_db_core::Db> {
    let path = resolve_path(args)?;
    if !path.is_file() {
        return Err(Error::DatabaseNotFound(path));
    }

    anlg_db_core::Db::connect_local_read_only(&path)
        .await
        .map_err(|error| Error::operation("open database", error.to_string()))
}

pub async fn open_write(args: &Args) -> Result<anlg_db_core::Db> {
    let path = resolve_path(args)?;
    if !path.is_file() {
        return Err(Error::DatabaseNotFound(path));
    }

    anlg_db_core::Db::connect_local_read_write(&path)
        .await
        .map_err(|error| Error::operation("open database for writes", error.to_string()))
}

pub(crate) fn resolve_path(args: &Args) -> Result<PathBuf> {
    Ok(resolve_source(args)?.into_path())
}

// Woher der Pfad stammt. `doctor` zeigt das an: bei einer Abweichung ist die
// erste Frage, welche Regel gegriffen hat, und ohne diese Auskunft raet man.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathSource {
    Flag(PathBuf),
    Base(PathBuf),
    AppHint(PathBuf),
    // Der Bauart-Standard, und dazu der Grund, falls ein Hinweis dafuer
    // verworfen wurde. Ohne diesen Grund sieht ein abgewiesener Hinweis genauso
    // aus wie gar keiner -- und das eine Ereignis, fuer das die Pruefung
    // gebaut ist, waere stumm.
    BuildDefault(PathBuf, Option<String>),
}

impl PathSource {
    pub(crate) fn path(&self) -> &Path {
        match self {
            Self::Flag(path)
            | Self::Base(path)
            | Self::AppHint(path)
            | Self::BuildDefault(path, _) => path,
        }
    }

    pub(crate) fn into_path(self) -> PathBuf {
        match self {
            Self::Flag(path)
            | Self::Base(path)
            | Self::AppHint(path)
            | Self::BuildDefault(path, _) => path,
        }
    }

    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Flag(_) => "--db-path",
            Self::Base(_) => "--base",
            Self::AppHint(_) => "hint from the desktop app",
            Self::BuildDefault(..) => "this build's default folder",
        }
    }

    pub(crate) fn rejected_hint(&self) -> Option<&str> {
        match self {
            Self::BuildDefault(_, reason) => reason.as_deref(),
            _ => None,
        }
    }
}

// Nur der `mcp`-Befehl ruft das auf (s. lib.rs::run). Jeder andere Befehl
// wird von einem Menschen getippt, der die Zieldatei gerade vor sich hat und
// einen falschen Pfad sofort sieht; ein MCP-Server dagegen laeuft
// unbeaufsichtigt im Hintergrund, einmal konfiguriert und dann nie wieder
// angesehen. Ein --db-path oder --base, das nicht auf einen Ordner dieses
// Forks zeigt, darf den Server deshalb erst gar nicht starten -- sonst haette
// ein vertippter Pfad einen zweiten, unbeaufsichtigten Prozess schreibend auf
// eine fremde, produktive Installation losgelassen (7,6 GB echte
// Nutzerdaten im Fall einer anarlog/Hyprnote-Installation).
//
// "Fremd" ist hier absichtlich die gleiche Pruefung wie bei einem Hinweis der
// Desktop-App (`is_own_app_folder`, mit `canonicalize` gegen einen Symlink,
// der sich als eigener Ordner tarnt) -- kein neues, zweites Kriterium.
// AppHint- und BuildDefault-Pfade sind dadurch bereits durch Konstruktion
// eigene Ordner; der Riegel greift praktisch nur bei Flag/Base, prueft aber
// bewusst uniform ueber alle vier Quellen, statt sich auf diese Annahme zu
// verlassen.
pub(crate) fn ensure_mcp_may_open(source: &PathSource) -> Result<()> {
    let path = source.path();
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    // Aufgeloest wird die DATEI, nicht nur ihr Ordner. Die erste Fassung dieses
    // Riegels pruefte den Elternordner und liess damit genau den Weg offen, den
    // sie schliessen sollte: eine `app.db`, die selbst ein Symlink auf die
    // Datenbank einer fremden anarlog-Installation ist, liegt in einem
    // einwandfrei eigenen Ordner -- SQLite folgt dem Symlink trotzdem und
    // oeffnet die fremde Datei schreibend. Gefunden im Cross-Vendor-Audit vom
    // 09.09.2026, wenige Stunden nach dem Bau.
    //
    // Existiert die Datei noch nicht, gibt es nichts aufzuloesen und damit auch
    // keinen Symlink, der etwas verstecken koennte; dann traegt wie zuvor der
    // Ordner.
    let resolved = match path.canonicalize() {
        Ok(file) => match file.parent() {
            Some(dir) => dir.to_path_buf(),
            None => return Ok(()),
        },
        Err(_) => parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf()),
    };
    let Some(folder_name) = resolved.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    if is_own_app_folder(folder_name) {
        return Ok(());
    }

    Err(Error::operation(
        "start MCP server",
        format!(
            "refusing to open a database outside this fork's own installation: {} (via {}) resolves into '{folder_name}', which is not a Mitschnitt data folder. Point --db-path at this fork's own app.db, or start Mitschnitt once so its own database exists.",
            path.display(),
            source.label(),
        ),
    ))
}

pub(crate) fn resolve_source(args: &Args) -> Result<PathSource> {
    if let Some(path) = &args.db_path {
        return Ok(PathSource::Flag(path.clone()));
    }
    if let Some(base) = &args.base {
        return Ok(PathSource::Base(base.join("app.db")));
    }

    // Der Hinweis der Desktop-App steht vor der eigenen Bauart-Regel: die App
    // kennt ihren Datenordner, die CLI kann ihn aus ihrer eigenen Bauart nur
    // erraten -- und lag falsch, sobald beide in verschiedenen Bauarten
    // vorlagen. Siehe anlg_storage::global::read_data_dir_hint fuer die
    // Pruefung, die einen Hinweis auf eine fremde Installation verwirft.
    let hint = managed_dir()
        .map(|dir| read_data_dir_hint(&dir))
        .unwrap_or(DataDirHint::Absent);
    let rejected = match hint {
        DataDirHint::Found(base) => return Ok(PathSource::AppHint(base.join("app.db"))),
        DataDirHint::Rejected(reason) => Some(reason),
        DataDirHint::Absent => None,
    };

    let data_dir = dirs::data_dir().ok_or_else(|| {
        Error::operation("resolve database path", "data directory is unavailable")
    })?;
    Ok(PathSource::BuildDefault(
        resolve_default_path(&data_dir),
        rejected,
    ))
}

// Der Ordner, in dem die verwaltete Binaerdatei liegt. Die App legt den
// Hinweis genau dort ab (embedded_cli::install_managed_cli).
//
// `canonicalize` ist der ganze Punkt: aufgerufen wird die CLI ueber den
// Symlink ~/.local/bin/mitschnitt, und `current_exe` gibt auf macOS GENAU
// DIESEN Symlink zurueck, nicht sein Ziel (gemessen 2026-09-09). Ohne die
// Aufloesung landet die Suche in ~/.local/bin, wo nie ein Hinweis liegt --
// der Normalfall waere also stumm gescheitert.
//
// Laeuft die CLI direkt aus dem Bauordner, liegt dort kein Hinweis und die
// Bauart-Regel greift. So gedacht.
fn managed_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let resolved = exe.canonicalize().unwrap_or(exe);
    resolved.parent().map(|parent| parent.to_path_buf())
}

fn resolve_default_path(data_dir: &Path) -> PathBuf {
    let command_name = std::env::args_os()
        .next()
        .and_then(|path| Path::new(&path).file_name().map(|name| name.to_owned()));
    resolve_default_path_for_command(data_dir, command_name.as_deref())
}

// The CLI resolves its database exactly like the desktop app does, through the
// shared folder rule in anlg_storage.
//
// Upstream searched a chain of channel folders instead and fell through to
// `hyprnote/app.db` and then `com.hyprnote.stable/app.db` whenever the earlier
// candidates were missing. Under the fork's own command name none of the
// channel arms match, so on a machine with a productive anarlog install that
// chain resolved to that install's live database -- a foreign 438 MB SQLite
// file in WAL mode, opened by a second process. The desktop app installs this
// CLI on every startup, so the chain was reachable without any user action.
//
// `command_name` is kept in the signature: the caller derives it from argv[0],
// and dropping it would change the public shape of the module for no gain.
fn resolve_default_path_for_command(data_dir: &Path, _command_name: Option<&OsStr>) -> PathBuf {
    let app_folder =
        anlg_storage::global::resolve_app_folder(APP_BUNDLE_ID, cfg!(debug_assertions));

    data_dir.join(app_folder).join("app.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regression guard. Upstream walked a chain of candidate folders and fell
    // through to `hyprnote/app.db` and `com.hyprnote.stable/app.db` whenever
    // its own candidates were missing, which on this machine is the live
    // database of a productive anarlog install. With all three foreign
    // candidates present and the fork's own folder absent, the old chain
    // returned one of them; this test fails if that behaviour ever returns.
    #[test]
    fn never_resolves_into_an_upstream_installation() {
        let dir = tempfile::tempdir().unwrap();
        let foreign = ["hyprnote", "anarlog", "com.hyprnote.stable"];
        for folder in foreign {
            let path = dir.path().join(folder).join("app.db");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap();
        }

        let resolved = resolve_default_path_for_command(dir.path(), Some(OsStr::new("mitschnitt")));

        for folder in foreign {
            assert_ne!(
                resolved,
                dir.path().join(folder).join("app.db"),
                "resolved into the {folder} folder of a foreign installation"
            );
        }
        assert!(
            !resolved.is_file(),
            "resolved onto an existing foreign database: {}",
            resolved.display()
        );
    }

    // argv[0] must not steer the lookup any more: the desktop app installs this
    // CLI under the fork's name, but a copy or symlink under any other name has
    // to keep pointing at the fork's own database.
    #[test]
    fn resolves_into_the_fork_folder_for_every_command_name() {
        let dir = tempfile::tempdir().unwrap();
        let expected = dir
            .path()
            .join(anlg_storage::global::resolve_app_folder(
                APP_BUNDLE_ID,
                cfg!(debug_assertions),
            ))
            .join("app.db");

        for name in [
            "mitschnitt",
            "anarlog",
            "anarlog-dev",
            "anarlog-staging",
            "",
        ] {
            assert_eq!(
                resolve_default_path_for_command(dir.path(), Some(OsStr::new(name))),
                expected
            );
        }
        assert_eq!(resolve_default_path_for_command(dir.path(), None), expected);
    }

    // Der Riegel aus Punkt 1: ein eigener Ordner laeuft durch, egal welche
    // PathSource-Quelle ihn traegt -- die Verschaerfung darf den Normalfall
    // nicht treffen.
    #[test]
    fn mcp_guard_accepts_a_path_into_this_forks_own_folder() {
        let dir = tempfile::tempdir().unwrap();
        let own = dir.path().join("Mitschnitt");
        std::fs::create_dir_all(&own).unwrap();

        assert!(ensure_mcp_may_open(&PathSource::Flag(own.join("app.db"))).is_ok());
        assert!(ensure_mcp_may_open(&PathSource::Base(own.join("app.db"))).is_ok());
    }

    // Der eigentliche Fall, fuer den der Riegel existiert: ein Pfad in eine
    // fremde, produktive anarlog/Hyprnote-Installation wird abgelehnt, mit
    // einer Meldung, die den fremden Ordner nennt statt still zu scheitern.
    #[test]
    fn mcp_guard_refuses_a_path_into_a_foreign_installation() {
        let dir = tempfile::tempdir().unwrap();
        let foreign = dir.path().join("hyprnote");
        std::fs::create_dir_all(&foreign).unwrap();

        let error = ensure_mcp_may_open(&PathSource::Flag(foreign.join("app.db"))).unwrap_err();
        assert!(
            error.to_string().contains("hyprnote"),
            "die Meldung nennt nicht den fremden Ordner: {error}"
        );
    }

    // Derselbe Zahn wie bei read_data_dir_hint: ein Symlink, der "Mitschnitt"
    // heisst, aber auf eine fremde Installation zeigt, darf nicht durch die
    // reine Namenspruefung schluepfen.
    #[test]
    fn mcp_guard_resolves_a_disguised_symlink_before_checking_the_name() {
        let dir = tempfile::tempdir().unwrap();
        let foreign = dir.path().join("hyprnote");
        std::fs::create_dir_all(&foreign).unwrap();
        let disguised = dir.path().join("Mitschnitt");
        std::os::unix::fs::symlink(&foreign, &disguised).unwrap();

        let error = ensure_mcp_may_open(&PathSource::Flag(disguised.join("app.db"))).unwrap_err();
        assert!(
            error.to_string().contains("hyprnote"),
            "die Meldung nennt nicht den fremden Ordner: {error}"
        );
    }

    // Der Weg, den die erste Fassung offen liess: nicht der ORDNER ist getarnt,
    // sondern die DATEI. Sie liegt in einem einwandfrei eigenen Ordner und
    // zeigt als Symlink auf die Datenbank einer fremden Installation -- SQLite
    // folgt ihm und oeffnet die fremde Datei. Der Ordner-Test oben laesst genau
    // das durch, deshalb steht dieser hier daneben.
    #[test]
    fn mcp_guard_refuses_a_database_file_that_links_into_a_foreign_installation() {
        let dir = tempfile::tempdir().unwrap();

        let foreign = dir.path().join("hyprnote");
        std::fs::create_dir_all(&foreign).unwrap();
        let foreign_db = foreign.join("app.db");
        std::fs::write(&foreign_db, b"nicht unsere Datenbank").unwrap();

        let own = dir.path().join(anlg_storage::global::APP_BUNDLE_ID);
        std::fs::create_dir_all(&own).unwrap();
        let disguised_db = own.join("app.db");
        std::os::unix::fs::symlink(&foreign_db, &disguised_db).unwrap();

        let error = ensure_mcp_may_open(&PathSource::Flag(disguised_db)).unwrap_err();
        assert!(
            error.to_string().contains("hyprnote"),
            "einer Datenbank-Datei, die in eine fremde Installation zeigt, wurde gefolgt: {error}"
        );
    }

    // Gegenprobe zum Test darueber: eine echte Datei im eigenen Ordner muss
    // weiter durchkommen. Sonst schuetzt der Riegel, indem er alles verbietet.
    #[test]
    fn mcp_guard_accepts_a_real_database_file_in_the_own_folder() {
        let dir = tempfile::tempdir().unwrap();
        let own = dir.path().join(anlg_storage::global::APP_BUNDLE_ID);
        std::fs::create_dir_all(&own).unwrap();
        let db = own.join("app.db");
        std::fs::write(&db, b"unsere Datenbank").unwrap();

        assert!(ensure_mcp_may_open(&PathSource::Flag(db)).is_ok());
    }
}
