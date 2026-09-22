use anlg_mirror::{MirrorPaths, MirrorState, RecorderView, SyncOutcome};

/// The CLI runs in its own process and cannot ask the app's recorder whether a
/// meeting is still being recorded (Mitschnitt-Fork, F16d). Only the file-quiet
/// window applies here, which is what the mirror always did -- so `mitschnitt
/// mirror sync` while the app is recording is no worse than before, and no
/// better.
const CLI_RECORDER: RecorderView = RecorderView::Unavailable;

use crate::cli::{Args, MirrorCommand};
use crate::{Result, output};

/// Returns `false` when the mirror has gaps, so `mirror status` can exit
/// non-zero and be used as a check rather than only read by a human.
pub async fn run(
    db: &anlg_db_core::Db,
    paths: &MirrorPaths,
    command: MirrorCommand,
    json: bool,
) -> Result<bool> {
    match command {
        MirrorCommand::Sync { id, force } => {
            let rendered = match id {
                Some(id) => {
                    let outcome =
                        anlg_mirror::sync_session(db.pool(), paths, &id, force, &CLI_RECORDER)
                            .await
                            .map_err(mirror_error)?;
                    if outcome == SyncOutcome::SessionGone {
                        return Err(crate::Error::NotFound(format!("meeting '{id}'")));
                    }
                    let written = outcome == SyncOutcome::Written;
                    if json {
                        output::json(
                            "mirror.sync",
                            &serde_json::json!({
                                "root": paths.root,
                                "written": if written { vec![id.clone()] } else { vec![] },
                                "up_to_date": usize::from(!written),
                                "orphans": Vec::<String>::new(),
                            }),
                            None,
                        )?
                    } else if written {
                        format!("Mirrored '{id}' into {}.", paths.root.display())
                    } else {
                        format!("'{id}' is already mirrored and up to date.")
                    }
                }
                None => {
                    let report = anlg_mirror::sync_all(db.pool(), paths, force, &CLI_RECORDER)
                        .await
                        .map_err(mirror_error)?;
                    let complete = report.failed.is_empty();
                    let rendered = if json {
                        output::json(
                            "mirror.sync",
                            &serde_json::json!({
                                "root": paths.root,
                                "written": report.written,
                                "up_to_date": report.up_to_date,
                                "orphans": report.orphans,
                                "failed": report
                                    .failed
                                    .iter()
                                    .map(|(id, reason)| serde_json::json!({
                                        "session_id": id,
                                        "reason": reason,
                                    }))
                                    .collect::<Vec<_>>(),
                                "complete": complete,
                            }),
                            None,
                        )?
                    } else {
                        render_sync(paths, &report)
                    };
                    // A pass that could not mirror every meeting exits non-zero,
                    // the same way `mirror status` does: a partial sync that
                    // reports success is how a broken mirror stays unnoticed.
                    output::emit(&rendered);
                    return Ok(complete);
                }
            };
            output::emit(&rendered);
            Ok(true)
        }
        MirrorCommand::Status => {
            let report = anlg_mirror::audit(db.pool(), paths)
                .await
                .map_err(mirror_error)?;

            let rendered = if json {
                output::json(
                    "mirror.status",
                    &serde_json::json!({
                        "root": paths.root,
                        "sessions": report
                            .sessions
                            .iter()
                            .map(|entry| serde_json::json!({
                                "session_id": entry.session_id,
                                "title": entry.title,
                                "state": state_name(&entry.state),
                                "detail": state_detail(&entry.state),
                            }))
                            .collect::<Vec<_>>(),
                        "orphans": report.orphans,
                        "complete": !report.has_gaps(),
                    }),
                    None,
                )?
            } else {
                render_status(paths, &report)
            };
            output::emit(&rendered);
            Ok(!report.has_gaps())
        }
    }
}

/// The mirror belongs to the database it mirrors -- including when the caller
/// pointed the CLI at a different database with `--db-path`, so a test or a
/// copy never writes into the real mirror.
///
/// The readable root is whatever that database says it is (F16c). Reading the
/// setting rather than assuming the default place is what keeps `mirror sync`
/// and `mirror status` honest once the folders have moved to a synced folder;
/// without it the CLI would rebuild every meeting back at the old place and
/// report a gap that only it can see.
pub async fn resolve_paths(args: &Args, db: &anlg_db_core::Db) -> Result<MirrorPaths> {
    let base = app_data_dir(args)?;
    match anlg_mirror::read_root_setting(db.pool())
        .await
        .map_err(mirror_error)?
    {
        Some(root) => Ok(MirrorPaths::with_root(root, &base)),
        None => Ok(MirrorPaths::for_app_data_dir(&base)),
    }
}

fn app_data_dir(args: &Args) -> Result<std::path::PathBuf> {
    let db_path = crate::db::resolve_path(args)?;
    db_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| {
            crate::Error::operation("resolve mirror path", "database path has no parent folder")
        })
}

fn mirror_error(error: anlg_mirror::Error) -> crate::Error {
    crate::Error::operation("mirror meetings", error.to_string())
}

fn render_sync(paths: &MirrorPaths, report: &anlg_mirror::SyncReport) -> String {
    let mut lines = vec![format!(
        "Mirror at {}: {} written, {} already current.",
        paths.root.display(),
        report.written.len(),
        report.up_to_date
    )];
    for (session_id, reason) in &report.failed {
        lines.push(format!("  could not mirror {session_id}: {reason}"));
    }
    for orphan in &report.orphans {
        lines.push(format!(
            "  orphaned folder (no such meeting, left untouched): {orphan}"
        ));
    }
    lines.join("\n")
}

fn render_status(paths: &MirrorPaths, report: &anlg_mirror::AuditReport) -> String {
    let mut lines = vec![format!("Mirror at {}", paths.root.display())];

    let gaps: Vec<_> = report.gaps().collect();
    if gaps.is_empty() && report.orphans.is_empty() {
        lines.push(format!(
            "All {} meetings are mirrored and current.",
            report.sessions.len()
        ));
        return lines.join("\n");
    }

    lines.push(format!(
        "{} of {} meetings need attention.",
        gaps.len(),
        report.sessions.len()
    ));
    for entry in gaps {
        let title = if entry.title.trim().is_empty() {
            "Untitled"
        } else {
            entry.title.trim()
        };
        let detail = state_detail(&entry.state)
            .map(|detail| format!(" ({detail})"))
            .unwrap_or_default();
        lines.push(format!(
            "  {:<10} {}  {}{}",
            state_name(&entry.state),
            entry.session_id,
            title,
            detail
        ));
    }
    for orphan in &report.orphans {
        lines.push(format!("  orphaned  {orphan}"));
    }
    lines.push("Run `mitschnitt mirror sync` to close the gaps.".to_string());
    lines.join("\n")
}

fn state_name(state: &MirrorState) -> &'static str {
    match state {
        MirrorState::Current => "current",
        MirrorState::Missing => "missing",
        MirrorState::Stale => "stale",
        MirrorState::Unreadable(_) => "unreadable",
    }
}

fn state_detail(state: &MirrorState) -> Option<&str> {
    match state {
        MirrorState::Unreadable(reason) => Some(reason.as_str()),
        _ => None,
    }
}
