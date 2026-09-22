use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "mitschnitt",
    version,
    about = "Den lesbaren Markdown-Spiegel von Mitschnitt pruefen und nachziehen"
)]
pub struct Args {
    #[arg(
        long,
        global = true,
        env = "MITSCHNITT_BASE",
        hide_env_values = true,
        value_name = "DIR"
    )]
    pub base: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        env = "MITSCHNITT_DB_PATH",
        hide_env_values = true,
        value_name = "FILE"
    )]
    pub db_path: Option<PathBuf>,

    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

impl Args {
    pub fn needs_write(&self) -> bool {
        matches!(
            self.command,
            Command::Proposals {
                command: ProposalCommand::Create { .. } | ProposalCommand::Decline { .. },
            } | Command::Mcp
        )
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check the local CLI and database connection without changing data
    Doctor,
    /// Serve Mitschnitt meeting data to a local MCP client over stdio
    Mcp,
    /// Keep the readable Markdown mirror next to the database in step
    Mirror {
        #[command(subcommand)]
        command: MirrorCommand,
    },
    /// Propose meeting edits for desktop review
    Proposals {
        #[command(subcommand)]
        command: ProposalCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum MirrorCommand {
    /// Write or refresh the readable mirror for every meeting, or just one
    Sync {
        #[arg(long = "id", value_name = "MEETING_ID")]
        id: Option<String>,
        #[arg(long, help = "Rewrite even when the mirror is already current")]
        force: bool,
    },
    /// Report which meetings have no mirror or a mirror that fell behind
    Status,
}

#[derive(Debug, Subcommand)]
pub enum ProposalCommand {
    /// Stage a summary or memo replacement for desktop review
    Create {
        #[arg(long = "meeting")]
        meeting_id: String,
        #[arg(long, value_enum)]
        kind: ProposalKind,
        #[arg(long = "target")]
        target_id: Option<String>,
        #[arg(long, required_unless_present = "content_file")]
        content: Option<String>,
        #[arg(long, value_name = "FILE", required_unless_present = "content")]
        content_file: Option<PathBuf>,
    },
    /// List staged meeting proposals
    List {
        #[arg(long = "meeting")]
        meeting_id: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..=200), help = "Maximum results (1-200)")]
        limit: u32,
        #[arg(long, default_value_t = 0, help = "Number of results to skip")]
        offset: u32,
    },
    /// Show one proposal and its unified diff
    Show { id: String },
    /// Decline a pending proposal without changing the meeting
    Decline { id: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ProposalKind {
    Summary,
    Memo,
}

impl ProposalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Memo => "memo",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn help_only_exposes_the_local_commands() {
        let help = Args::command().render_long_help().to_string();
        assert!(help.contains("mirror"));
        assert!(help.contains("proposals"));
        assert!(help.contains("doctor"));
        // Der MCP-Server ist wieder da (ZICK-nachfolgend): `mitschnitt mcp`
        // bedient den Knopf in den Desktop-Einstellungen und muss deshalb in
        // der Befehlsflaeche stehen.
        assert!(help.contains("mcp"));

        // Der Fork hat weiterhin kein Konto und keine Cloud-Gespraechsliste.
        // Faellt eines davon je wieder in die Oberflaeche zurueck, faellt
        // dieser Test.
        for gone in ["auth", "meetings"] {
            assert!(
                !help.contains(gone),
                "`{gone}` ist wieder in der Befehlsflaeche"
            );
        }
    }

    #[test]
    fn parses_mirror_sync_filters() {
        let args = Args::parse_from(["mitschnitt", "--json", "mirror", "sync", "--id", "m-1"]);

        assert!(args.json);
        let Command::Mirror { command } = args.command else {
            panic!("expected mirror command");
        };
        assert!(matches!(
            command,
            MirrorCommand::Sync {
                id: Some(ref id),
                force: false,
            } if id == "m-1"
        ));
    }

    #[test]
    fn proposal_create_needs_content_from_somewhere() {
        assert!(
            Args::try_parse_from([
                "mitschnitt",
                "proposals",
                "create",
                "--meeting",
                "m-1",
                "--kind",
                "summary",
            ])
            .is_err()
        );
    }

    // Die beiden schreibenden Vorschlags-Befehle oeffnen die Datenbank
    // schreibend. `mcp` gehoert dazu, obwohl es selbst kein Vorschlags-Befehl
    // ist: seine propose_*/decline_proposal-Werkzeuge schreiben in
    // session_proposals, und eine read-only-Verbindung wuerde das erst beim
    // ersten echten Aufruf verraten statt beim Start. Alles andere bleibt
    // lesend.
    #[test]
    fn writing_commands_and_mcp_open_for_writes() {
        let needs_write = |argv: &[&str]| Args::parse_from(argv).needs_write();

        assert!(needs_write(&[
            "mitschnitt",
            "proposals",
            "create",
            "--meeting",
            "m-1",
            "--kind",
            "summary",
            "--content",
            "x"
        ]));
        assert!(needs_write(&["mitschnitt", "proposals", "decline", "p-1"]));
        assert!(needs_write(&["mitschnitt", "mcp"]));
        assert!(!needs_write(&["mitschnitt", "proposals", "list"]));
        assert!(!needs_write(&["mitschnitt", "mirror", "sync"]));
        assert!(!needs_write(&["mitschnitt", "mirror", "status"]));
        assert!(!needs_write(&["mitschnitt", "doctor"]));
    }
}
