#![forbid(unsafe_code)]

mod cli;
mod commands;
mod db;
mod error;
mod mcp;
mod output;

pub use cli::Args;
pub use error::{Error, Result};
pub use output::JSON_SCHEMA_VERSION;

pub async fn run(args: Args) -> Result<u8> {
    if matches!(&args.command, cli::Command::Doctor) {
        let ready = commands::doctor::run(&args, args.json).await?;
        return Ok(if ready { 0 } else { 1 });
    }

    // Nur `mcp` bekommt den Riegel gegen fremde Installationen (Details in
    // db::ensure_mcp_may_open) -- jeder andere Befehl behaelt seine volle
    // --db-path/--base-Freiheit unveraendert.
    if matches!(&args.command, cli::Command::Mcp) {
        db::ensure_mcp_may_open(&db::resolve_source(&args)?)?;
    }

    let json = args.json;
    let db = if args.needs_write() {
        db::open_write(&args).await?
    } else {
        db::open(&args).await?
    };
    // Resolved before `args.command` is moved into the match below. Needs the
    // database, because the readable root is a setting stored in it.
    let mirror_paths = commands::mirror::resolve_paths(&args, &db).await?;

    match args.command {
        cli::Command::Doctor => unreachable!("doctor returns before opening the database"),
        cli::Command::Mcp => mcp::serve(std::sync::Arc::new(db)).await?,
        cli::Command::Mirror { command } => {
            let complete = commands::mirror::run(&db, &mirror_paths, command, json).await?;
            return Ok(u8::from(!complete));
        }
        cli::Command::Proposals { command } => commands::proposals::run(&db, command, json).await?,
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn doctor_returns_nonzero_status_when_database_is_not_ready() {
        let dir = tempfile::tempdir().unwrap();
        let status = run(Args {
            base: None,
            db_path: Some(dir.path().join("missing.db")),
            json: true,
            command: cli::Command::Doctor,
        })
        .await
        .unwrap();

        assert_eq!(status, 1);
    }

    // `mirror status` is meant to be usable as a check, so its exit code has to
    // carry the answer: non-zero while a meeting is unmirrored, zero once the
    // mirror is complete.
    #[tokio::test]
    async fn mirror_status_exits_nonzero_until_sync_closes_the_gap() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("app.db");
        let db = anlg_db_core::Db::connect_local_plain(&db_path)
            .await
            .unwrap();
        anlg_db_app::prepare_schema(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, title, started_at) VALUES ('meeting-1', 'Planning', '2026-07-13')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO session_documents (id, session_id, kind, body_format, body)
             VALUES ('meeting-1', 'meeting-1', 'note', 'markdown', 'Decide the launch date.')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        db.pool().close().await;

        let status = |command| {
            let db_path = db_path.clone();
            async move {
                run(Args {
                    base: None,
                    db_path: Some(db_path),
                    json: false,
                    command,
                })
                .await
                .unwrap()
            }
        };

        assert_eq!(
            status(cli::Command::Mirror {
                command: cli::MirrorCommand::Status
            })
            .await,
            1
        );
        assert_eq!(
            status(cli::Command::Mirror {
                command: cli::MirrorCommand::Sync {
                    id: None,
                    force: false
                }
            })
            .await,
            0
        );
        assert_eq!(
            status(cli::Command::Mirror {
                command: cli::MirrorCommand::Status
            })
            .await,
            0
        );

        let mirrored = std::fs::read_dir(dir.path().join("mirror"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let summary = std::fs::read_to_string(mirrored.join("summary.md")).unwrap();
        assert!(summary.contains("Decide the launch date."), "{summary}");
    }

    // F16c. Once the readable folders live somewhere else, the CLI has to write
    // there too. A CLI that keeps assuming the default place would rebuild every
    // meeting at the old one and report a gap only it can see -- two places
    // again, this time between the app and its own command line.
    #[tokio::test]
    async fn mirror_sync_follows_the_chosen_folder() {
        let dir = tempfile::tempdir().unwrap();
        let chosen = dir.path().join("Nextcloud").join("Mitschnitt");
        let db_path = dir.path().join("app.db");
        let db = anlg_db_core::Db::connect_local_plain(&db_path)
            .await
            .unwrap();
        anlg_db_app::prepare_schema(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, title, started_at) VALUES ('meeting-1', 'Planning', '2026-07-13')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query("INSERT INTO app_settings (id, value_json) VALUES (?, ?)")
            .bind(anlg_mirror::ROOT_SETTING_KEY)
            .bind(format!("\"{}\"", chosen.display()))
            .execute(db.pool())
            .await
            .unwrap();
        db.pool().close().await;

        let code = run(Args {
            base: None,
            db_path: Some(db_path),
            json: false,
            command: cli::Command::Mirror {
                command: cli::MirrorCommand::Sync {
                    id: None,
                    force: false,
                },
            },
        })
        .await
        .unwrap();

        assert_eq!(code, 0);
        assert_eq!(std::fs::read_dir(&chosen).unwrap().count(), 1);
        assert!(
            !dir.path().join("mirror").exists(),
            "the CLI wrote to the default place as well"
        );
    }

    #[tokio::test]
    async fn proposal_create_lists_and_declines_without_changing_the_note() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("app.db");
        let db = anlg_db_core::Db::connect_local_plain(&db_path)
            .await
            .unwrap();
        anlg_db_app::prepare_schema(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, title, started_at) VALUES ('meeting-1', 'Planning', '2026-07-13')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO session_documents (id, session_id, kind, body_format, body)
             VALUES
             ('meeting-1', 'meeting-1', 'note', 'markdown', 'Original memo'),
             ('summary-1', 'meeting-1', 'summary', 'markdown', 'Original summary')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        db.pool().close().await;

        let run_command = |command| {
            let db_path = db_path.clone();
            async move {
                run(Args {
                    base: None,
                    db_path: Some(db_path),
                    json: true,
                    command,
                })
                .await
                .unwrap()
            }
        };

        run_command(cli::Command::Proposals {
            command: cli::ProposalCommand::Create {
                meeting_id: "meeting-1".to_string(),
                kind: cli::ProposalKind::Summary,
                target_id: None,
                content: Some("Revised summary".to_string()),
                content_file: None,
            },
        })
        .await;

        let read = anlg_db_core::Db::connect_local_read_only(&db_path)
            .await
            .unwrap();
        let (created_id, pending): (String, (String, String)) = {
            let row: (String, String, String) =
                sqlx::query_as("SELECT id, status, proposed_markdown FROM session_proposals")
                    .fetch_one(read.pool())
                    .await
                    .unwrap();
            (row.0, (row.1, row.2))
        };
        let note: String =
            sqlx::query_scalar("SELECT body FROM session_documents WHERE id = 'summary-1'")
                .fetch_one(read.pool())
                .await
                .unwrap();
        read.pool().close().await;

        assert_eq!(pending, ("pending".to_string(), "Revised summary".to_string()));
        assert_eq!(note, "Original summary");

        // Trotz seines Namens rief dieser Test bisher weder List noch
        // Decline auf -- nur Create, direkt per SQL nachgemessen. Beide
        // Befehle laufen jetzt tatsaechlich ueber run(), damit der Name
        // stimmt.
        let list_exit_code = run_command(cli::Command::Proposals {
            command: cli::ProposalCommand::List {
                meeting_id: Some("meeting-1".to_string()),
                status: None,
                limit: 20,
                offset: 0,
            },
        })
        .await;
        assert_eq!(list_exit_code, 0);

        let decline_exit_code = run_command(cli::Command::Proposals {
            command: cli::ProposalCommand::Decline {
                id: created_id.clone(),
            },
        })
        .await;
        assert_eq!(decline_exit_code, 0);

        let read = anlg_db_core::Db::connect_local_read_only(&db_path)
            .await
            .unwrap();
        let status: String =
            sqlx::query_scalar("SELECT status FROM session_proposals WHERE id = ?")
                .bind(&created_id)
                .fetch_one(read.pool())
                .await
                .unwrap();
        let note_after_decline: String =
            sqlx::query_scalar("SELECT body FROM session_documents WHERE id = 'summary-1'")
                .fetch_one(read.pool())
                .await
                .unwrap();
        read.pool().close().await;

        assert_eq!(status, "declined");
        assert_eq!(note_after_decline, "Original summary");
    }
}
