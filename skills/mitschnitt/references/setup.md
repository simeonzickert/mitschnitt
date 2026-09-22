# Setup

There is no cloud service, no account, and no hosted API in this fork. Nothing
needs to be enabled or signed into. The CLI talks to the local database only.

## Install the CLI

Open **Mitschnitt → Settings → Developers** and select **Install**. The desktop
app installs the command as:

- macOS and Linux: `~/.local/bin/mitschnitt`
- Windows: `%LOCALAPPDATA%\Mitschnitt\bin\mitschnitt.exe`

Run the Mitschnitt desktop app once so its local database exists. After that
the CLI works while the app is closed.

To build from source instead:

```bash
cargo install --locked --path apps/cli
mitschnitt --version
```

## Confirm it works

```bash
mitschnitt --json doctor
```

If that reports the database is missing, either the app has not run yet or the
data lives somewhere else. Use `--db-path FILE` or `--base DIR` (or the
`MITSCHNITT_DB_PATH` / `MITSCHNITT_BASE` environment variables) only in the
second case.

## Reading without the CLI

The `mirror/` folder beside the database holds every meeting as plain Markdown.
For pure reading, that is usually enough and needs no installation at all.
