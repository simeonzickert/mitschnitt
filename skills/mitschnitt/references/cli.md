# CLI commands

The command is `mitschnitt`. Use `--json` for agent-readable output.

The full command surface is three groups: `doctor`, `mirror`, and `proposals`.
There is no `auth`, no `meetings`, and no `mcp` command in this fork.

## Check the connection

```bash
mitschnitt --json doctor
```

`doctor` reports whether the CLI can reach the local database without changing
anything.

## Readable Markdown mirror

The mirror lives in a `mirror/` folder next to the database and is kept in step
by the desktop app. It is the easiest way to read meeting content.

```bash
mitschnitt --json mirror status
mitschnitt --json mirror sync
mitschnitt --json mirror sync --id MEETING_ID
mitschnitt --json mirror sync --id MEETING_ID --force
```

`mirror status` reports meetings with no mirror or a mirror that fell behind.
`mirror sync` writes files only; it never changes a meeting. `--force` rewrites
even when the mirror is already current.

## Proposals

A proposal stages a replacement for review. It does not modify the meeting.

```bash
mitschnitt --json proposals list
mitschnitt --json proposals list --meeting MEETING_ID --status pending --limit 20 --offset 0
mitschnitt --json proposals show PROPOSAL_ID
mitschnitt --json proposals create --meeting MEETING_ID --kind summary --content "Replacement markdown"
mitschnitt --json proposals create --meeting MEETING_ID --kind memo --content-file ./memo.md
mitschnitt --json proposals decline PROPOSAL_ID
```

`--kind` is `summary` or `memo`. Pass `--target` when several summaries exist.
`create` needs either `--content` or `--content-file`. `list` defaults to a
limit of 20 and accepts 1 to 200.

## Database location

The CLI resolves the database the same way the desktop app does. Override it
only when the data lives somewhere else:

```bash
mitschnitt --db-path /path/to/app.db --json doctor
mitschnitt --base /path/to/data-dir --json doctor
```

The same values can come from the environment as `MITSCHNITT_DB_PATH` and
`MITSCHNITT_BASE`.

## Output shape

JSON success responses contain `schema_version`, `command`, `data`, and an
optional `pagination` object. Continue from `pagination.next_offset` only when
you actually need more.
