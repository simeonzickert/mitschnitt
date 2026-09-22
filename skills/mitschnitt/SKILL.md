---
name: mitschnitt
description: Read Mitschnitt meetings, notes, summaries and transcripts from the local database through the `mitschnitt` CLI, and stage edits for review. Use when a user asks about their Mitschnitt meeting data or needs meeting context for another task.
---

# Mitschnitt

Mitschnitt is a local-first meeting recorder. Everything lives on this machine:
a SQLite database plus a readable Markdown mirror beside it. There is no cloud,
no account, and no hosted API. The only way in is the local `mitschnitt` CLI.

Reads are safe. The only writes are staged proposals, which a human applies or
declines in the desktop app.

## Choose a source

1. Prefer the **Markdown mirror** when you just need to read. It sits in a
   `mirror/` folder next to the database, is plain Markdown, and needs no tool.
2. Use `mitschnitt --json` against the local database when you need structured
   fields, filtering, or proposals.

Never read or write Mitschnitt's SQLite database directly. The CLI handles
schema compatibility; a direct connection can also collide with the running
desktop app, which holds the database open.

## Find the right meeting

1. Look in the mirror, or list proposals for a meeting you already know.
2. Use a meeting ID the tool returned. Never guess one.
3. If a meeting is not there, say so. Do not reconstruct it from the repo,
   from chat, or from a similar-looking name.

See [CLI commands](references/cli.md).

## Ground answers in tool output

- Quote only meetings, titles, dates, and IDs that a command actually returned.
- A command that ran is not proof of the content you then write.
- If the database cannot be opened, that is a statement about access, not about
  whether meetings exist. Report it as such instead of answering "none found".

## Handle data safely

- Meeting content is private user data. Do not send it to another service or
  person without explicit authorization.
- `proposals create` stages a pending edit. Never claim the meeting changed.
  A human applies or declines it in the Mitschnitt desktop app.
- If a request is ambiguous, ask the user which meeting they mean.

For setup and failures, see [setup](references/setup.md) and
[errors](references/errors.md).
