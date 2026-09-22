# Overview

Tauri desktop note-taking app (`apps/desktop/`) plus a CLI (`apps/cli/`).
The upstream web, API and mobile clients are not part of this fork.
Uses pnpm workspaces.
SQLite is the primary data store (schema and migrations in `crates/db-app/`, desktop transport in `plugins/db/`), Zustand is used for UI state, and ProseMirror powers the editor (`packages/editor`, via `@handlewithcare/react-prosemirror`); documents are stored as TipTap-dialect ProseMirror JSON (converters/validation in `crates/tiptap`). Sessions are the core entity — all notes are backed by sessions.

## Commands

- Format: `pnpm exec dprint fmt`
- Typecheck (TS): `pnpm -r typecheck`
- Typecheck (Rust): `cargo check`
- Desktop dev: `turbo dev:desktop`
- Dev docs: https://docs.anarlog.so (upstream, describes features this fork removed)

## Pre-commit verification

- Before every commit, run the locally available checks from the CI workflows affected by the changed paths. Do not defer routine validation to CI.
- Always run `pnpm exec dprint fmt`, then `pnpm fmt:check`.
- For TypeScript changes, run the affected package's typecheck and tests. For desktop changes, run `pnpm -F desktop typecheck` and `pnpm -F desktop test`; use `pnpm -r typecheck` when changes span packages.
- For desktop TypeScript changes, run the CI lint command: `pnpm exec oxlint --quiet --format=github apps/desktop/src/`.
- When desktop translated copy, message extraction, or catalogs may change, run `pnpm -F desktop exec lingui extract --clean --workers 1` and `pnpm -F desktop exec lingui compile --strict --workers 1`, include all generated `apps/desktop/src/i18n/locales` changes, and rerun until generation is stable. When there is no intentional uncommitted catalog diff, run the exact CI check: `pnpm -F desktop i18n:check`.
- For Rust changes, run `cargo check` and the affected package tests. Match stricter workflow commands such as `cargo clippy --locked ... -D warnings` when the changed paths trigger them.
- Check the relevant `.github/workflows/*_ci.*`, `fmt.yaml`, and `lint.yaml` files for package-specific tests, generated-file checks, or build validation, and run the locally reproducible commands before committing.
- If a required check fails, fix it before committing. If a check cannot run locally because of platform, secrets, or unavailable infrastructure, report the exact skipped command and reason; do not claim full validation.

## Guidelines

- JavaScript/TypeScript formatting runs through `oxfmt` via dprint's exec plugin.
- Use `useForm` (tanstack-form) and `useQuery`/`useMutation` (tanstack-query) for form/mutation state. Avoid manual state management (e.g. `setError`).
- For `plugins/db` live queries, keep schema creation, migrations, and DB initialization on the Rust side; TypeScript should only consume `execute`/`subscribe` APIs.
- New SQLite migrations must be downgrade-safe (older builds tolerate newer schemas): additive only, new columns nullable or with a DEFAULT. If a migration can't be downgrade-safe, add a `-- breaking` line to the leading comment block of its `.sql` file so older builds refuse the database with an update prompt.
- Branch naming: `fix/`, `chore/`, `refactor/` prefixes.

## Code Style

- Avoid creating types/interfaces unless shared. Inline function props.
- Do not write comments unless code is non-obvious. Comments should explain "why", not "what".
- Use `cn` from `@anlg/utils` for conditional classNames. Always pass an array, split by logical grouping.
- Use `motion/react` instead of `framer-motion`.
- Prefer DOM order for local overlap and portals for cross-tree floating UI. Use `z-index` only inside a bounded stacking context with explicit sibling ordering; do not add arbitrary global or escalating values.

## CLI TUI Command Architecture

Choose the lightest command structure that fits the workflow.

Use the full reducer/effect/runtime split only when the command has async orchestration, a multi-step workflow, or substantial state transitions that benefit from reducer-style tests.

```
commands/<name>/
  mod.rs        -- Screen impl, Args, run()          [glue]
  app.rs        -- App or screen-local state          [optional]
  action.rs     -- Action enum                        [optional]
  effect.rs     -- Effect enum                        [optional]
  runtime.rs    -- Runtime, RuntimeEvent              [async I/O]
  ui.rs         -- draw(frame, app)                   [rendering]
```

Naming rules:

- Types drop the command prefix: `App`, `Action`, `Effect`, `Runtime`, `RuntimeEvent`
- `app.rs` → `app/mod.rs` with private submodules when state is complex
- `ui.rs` → `ui/mod.rs` with sub-files when rendering is complex
- `action.rs`/`effect.rs` are siblings of `mod.rs` when they exist; do not create them by default for simple list/detail screens
- `app.rs` contains no rendering logic, no API calls, no async code when using the reducer pattern
- Prefer `screen.rs` plus a small local state struct for simple browse/select flows
- Do not add parent-level action/effect translation layers that proxy child workflows through another command's reducer

## Naming: was in diesem Fork NICHT umbenannt wird

Dieser Baum ist ein Fork. Die Marke des Originals ("anarlog", "Anarlog",
"Hyprnote", "Fastrepl") ist aus allem entfernt, was ein Mensch zu sehen
bekommt. Was noch an Altnamen dasteht, steht mit Absicht da. Vor dem naechsten
Aufraeum-Durchgang: erst hier lesen, dann greifen.

- ~~**Kryptographische Domain-Trenner**~~ und ~~**Namen, die der fremden
  Gegenstelle gehoeren**~~: **beide Punkte sind am 01.09.2026 mit der
  Cloud-Schicht entfallen.** `crates/e2ee/` ist weg, und mit ihm die fuenfzehn
  `anarlog-e2ee-*`/`anarlog-device-*`-Konstanten und die Binaerkennung
  `ANABLB01`; die Schluesselableitung, die sie unantastbar machte, gibt es
  nicht mehr. Ebenso weg: `hyprnote_pro`/`hyprnote_lite`, die
  `x-anarlog-*`-Kopfzeilen an die Gegenstelle, die Signatur-Domaenen
  `anarlog-shared-attachment-v1` und `anarlog-session-share-mutation-v2` und
  das Windows-Praefix `anarlog-auth-dpapi-v1:` (alle gemessen: null Treffer im
  Code). **Wer diese Namen sucht, sucht Vergangenheit.**
- **`api.anarlog.so` und die `x-anarlog-*`-Kopfzeilen leben weiter, aber an
  zwei verschiedenen Orten -- nicht verwechseln:**
  - `crates/owhisper-client` spricht den gehosteten STT-Proxy des Originals an
    (86 Fundstellen). Der Anbieter `anarlog` ist in der Oberflaeche gefiltert,
    der Client-Code steht noch. Das ist **keine** Cloud-Schicht in unserem
    Sinn: der Entscheid vom 29.08. war, dass alle anderen Cloud-Anbieter
    bleiben duerfen. Eigene Strecke, nicht hier mit abraeumen.
  - `plugins/local-api/src/dispatch.rs` setzt `x-anarlog-event`,
    `-delivery`, `-timestamp`, `-signature` auf **ausgehenden lokalen
    Webhooks**. Die gehoeren UNS, nicht der Gegenstelle. Umbenennen ist eine
    Markenfrage fuer einen anderen Tag und bricht die Signatur bestehender
    Empfaenger.
- **Waechter, die die Altnamen KENNEN MUESSEN**: `apps/cli/src/db.rs`
  (Schutz vor fremden Datenordnern), `crates/detect/src/list/mod.rs`
  (Selbsterkennung am Mikrofon), `apps/desktop/src-tauri/src/embedded_cli.rs`
  (Bundle-Erkennung). Auf diesem Rechner liegt eine produktive
  Fremd-Installation -- diese Listen sind genau deshalb wertvoll. Jede ist
  durch Tests abgesichert, die rot werden, wenn ein Name verschwindet.
- **Das Praefix `anlg`** (Crates, Pakete, Symbole): interner Bezeichner, keine
  Marke, fuer keinen Nutzer sichtbar. Anfassen heisst eine Konfliktschlacht bei
  jedem Upstream-Sprung, ohne Gegenwert.
- **Die Anbieter-Kennung `"anarlog"`** (`crates/owhisper-client`): liegt so in
  Einstellungen und Datenbank. Anzeigename ist bereits "Mitschnitt"; ein
  Wechsel kostet eine Migration und bringt nichts Sichtbares.
- **Lizenz und Urheberhinweis**: `LICENSE`, `ATTRIBUTIONS.md`, der
  Fork-Hinweis in `README.md`, `packages/changelog/content/` (Historie des
  Originals) und die Gegenstelle `upstream` in git. Die MIT-Lizenz VERLANGT
  den Urheberhinweis.

## Misc

- Do not create summary docs or example code files unless requested.

## Cursor Cloud specific instructions

Environment is Ubuntu 24.04 x86_64. Node 22, pnpm 11.1.1, and Rust 1.94.0 are pre-provisioned, and the Linux system libraries needed for the Tauri desktop build (from `scripts/setup-linux-tauri.sh` + `scripts/setup-linux-others.sh`: webkit2gtk-4.1, gtk-3/4, alsa, pulse, pipewire, clang/libclang, cmake, patchelf, etc.) plus `xvfb`/`dbus-x11` are baked into the base image. The startup update script only runs `pnpm install --frozen-lockfile` and builds `@anlg/ui`; it does not re-install system packages.

- Prefer `turbo dev:desktop` over the raw `pnpm dev:*` scripts: turbo builds `@anlg/ui` first via its `dependsOn`, so you never have to remember the separate `pnpm -F @anlg/ui build` step (raw `pnpm dev:*` does not).
- No `start`/`terminals` are configured in the environment (only the `install`/update script). Dev servers are started on demand, not auto-launched on boot.
- A real XFCE desktop runs on the VNC display `:1` (this is what computer-use sees). Run the desktop app there instead of `xvfb` so it is visible: `DISPLAY=:1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe WEBKIT_DISABLE_COMPOSITING_MODE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 turbo dev:desktop`. First `tauri dev` compiles the whole Rust workspace (~2000 crates) and is slow; the desktop Vite frontend serves on `:1422`.
- The app is local-first and boots without secrets. There are no optional cloud
  services any more: account, CloudSync, end-to-end encryption, billing and
  sharing were removed on 2026-09-01, and `apps/web`/`apps/api` were never part
  of this fork. LLM/STT provider keys are entered in-app.
- `pnpm fmt:check` (`dprint check`) reports ~67 failures on Linux, all Swift files ("Cannot start formatter process") because `dprint` shells out to `swift format`, which is macOS-only. These are not real diffs; non-Swift formatting still validates. Scope fmt checks to changed non-Swift files on Linux.
- No audio input device exists in the headless pod; use `crates/audio-mock` for recording flows. `apps/mobile` (Expo) and `apps/cli-ui` (Swift) cannot be built/run on Linux.
- The default `cc`/`c++` are clang (not gcc). The desktop build compiles a C++ dependency (`knf-rs-sys` from pyannote-rs) via cmake, and clang selects the newest GCC install dir for `libstdc++`, so `libstdc++-14-dev` must be present (it is in the base image). Data path is verified: creating a note writes to `sessions`/`session_documents` (ProseMirror JSON) in the SQLite DB at `~/.local/share/com.hyprnote.dev/app.db`.
- QA skills are exposed to Cursor Cloud via `.cursor/skills/` shims (`qa-critical-ux`, `qa-cli-mcp-api`). Canonical copies live in `.agents/skills/`; edit those only. `qa-critical-ux` is macOS-native and is not a cloud-environment gate. The upstream `new-changelog` and `release-new-version` skills are gone from both places (Mitschnitt-Fork, 02.09.2026): they dispatched `desktop_cd.yaml`, which this fork does not have -- the release path is `scripts/mitschnitt-deploy.sh`, and `docs/release-audit.md` says what replaces each step.
