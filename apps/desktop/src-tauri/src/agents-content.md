# Mitschnitt Desktop

This file is auto-generated on app startup.

## Meeting data

Use Mitschnitt's typed, read-only interfaces for meeting data.
Do not use `find`, `grep`, `rg`, filesystem crawling, or direct SQLite queries
to find or read meetings. The desktop app holds the database open; a direct
connection can collide with it.

Two ways in, both local. There is no cloud service and no account.

The readable Markdown mirror in the `mirror/` folder beside the database is the
simplest source for plain reading.

For structured output, use the Mitschnitt CLI with `--json`:

```sh
mitschnitt --json doctor
mitschnitt --json mirror status
mitschnitt --json mirror sync --id MEETING_ID
mitschnitt --json proposals list --meeting MEETING_ID
mitschnitt --json proposals show PROPOSAL_ID
```

The CLI discovers Mitschnitt's database from the platform application-data
directory. Use `--db-path ABSOLUTE_APP_DB` only when the user explicitly
provides a non-default database path; do not crawl the filesystem to find one.

Never guess a meeting ID. A proposal only stages an edit for review in the
desktop app; it never changes the meeting itself.
