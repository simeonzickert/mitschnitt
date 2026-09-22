# Contributing

Issues, pull requests, bug reports, and documentation fixes are welcome.

## Before you start

- Search existing issues and pull requests before starting a large change.
- Keep changes focused. Add tests for behavior that can regress.
- Never commit credentials, customer configuration, meeting content, or other private data.
- Use `skills/mitschnitt/` for CLI behavior. Use this file and the repository's `AGENTS.md` files for development guidance. Upstream's documentation site describes the original product, not this fork.

## Set up the repository

You need:

- Node.js 22 or later
- pnpm 11.1.1
- Rust 1.94.0
- The [Tauri v2 system dependencies](https://v2.tauri.app/start/prerequisites/)

On Debian or Ubuntu, install the supported toolchains and system packages with:

```bash
bash scripts/setup-linux.sh
```

Then install the workspace:

```bash
pnpm install --frozen-lockfile
```

The desktop app and website start without secrets for local-first workflows.

## Run the app

```bash
# Tauri desktop app
pnpm exec turbo dev:desktop
```

Turbo builds the shared UI packages first. There is no `dev:web` in this fork:
the upstream website lived in `apps/web`, which was never forked.

There are no optional cloud services. This fork talks to no backend of its own: the cloud
layer (account, CloudSync, end-to-end encryption, hosted STT/LLM, billing, sharing) was
removed on 2026-09-01. Notes, recording, transcription, summaries and the markdown mirror
all run on the machine. Provider credentials for third-party AI are entered in the app.

## Find the right code

| Path | Scope |
| --- | --- |
| `apps/desktop` | React desktop UI and Tauri application |
| `apps/cli` | CLI (`mitschnitt`) |
| `plugins/*` | Tauri plugin boundaries |
| `crates/*` | Rust libraries |
| `packages/*` | Shared TypeScript packages |
| `crates/db-app` | SQLite schema and migrations |
| `crates/mirror` | One-way markdown mirror of the database |
| `skills/mitschnitt` | Published CLI agent skill |

The upstream rows `apps/web`, `apps/api`, `apps/mobile`, `supabase` and `docs` are gone:
the web, API and mobile clients were never part of this fork, and the hosted database
schema went with the cloud layer on 2026-09-01.

Sessions are the core data entity. Notes, transcripts, and summaries are all backed by sessions. ProseMirror documents use the TipTap JSON dialect.

## Validate your change

Always format before committing:

```bash
pnpm exec dprint fmt
pnpm fmt:check
```

On Linux, the full format check cannot run the macOS-only Swift formatter. Run a scoped dprint check for every changed non-Swift path and report the skipped Swift check.

Run checks for every package you changed. Common commands include:

```bash
# Desktop TypeScript
pnpm -F desktop typecheck
pnpm -F desktop test
pnpm exec oxlint --quiet --format=github apps/desktop/src/

# Changes spanning TypeScript packages
pnpm -r typecheck

# Rust
cargo check
cargo test -p <affected-package>
```

For documentation changes:

```bash
pnpm exec dprint fmt 'docs/**/*'
pnpm exec dprint check 'docs/**/*'
cd docs
mint validate
mint broken-links --check-anchors --check-redirects
```

Check the affected workflow under `.github/workflows/` for stricter package-specific commands.

## Open a pull request

- Write the title as a specific action that states the intended outcome. Do not use a file name, ticket number, or a generic label as the title.
- Write the description yourself as a concise executive summary: on the labeled `Problem` and `Fix` lines, explain the problem, why it mattered, and how the change fixes it. Do not paste a generated commit log or a file-by-file recap.
- List the commands and manual checks you used to verify the change.
- CI enforces the title and labeled `Problem` / `Fix` summary for external contributors. Org members, collaborators, owners, and bots are not gated.

## Licensing and contribution boundary

Mitschnitt is a fork of [anarlog](https://github.com/fastrepl/anarlog). The whole repository is distributed under the [MIT License](LICENSE), copyright Fastrepl, Inc. The upstream `enterprise/` tree, which was commercially licensed, does not exist in this fork. By submitting a contribution you agree that it may be distributed under MIT. Only submit material you have the right to license this way.

Never include customer configuration, credentials, confidential material, or untracked third-party code. Before reusing third-party code or assets, record the upstream repository, the immutable revision, the copyright holder, the license, and whether the material was copied, modified, or only used as a behavioral reference. Keep that record beside the consuming code and add an entry to [ATTRIBUTIONS.md](ATTRIBUTIONS.md), which is shipped inside the application bundle. Do not add AGPL, GPL, SSPL, source-available, or unknown-license material.
