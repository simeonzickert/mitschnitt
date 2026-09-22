# Release audit

> **Mitschnitt-Fork (02.09.2026):** Die Release-Pipeline des Originals (`desktop_cd.yaml`, `desktop_publish.yaml`, `desktop_store_publish.yaml`, CrabNebula-Provenance) existiert in diesem Fork nicht mehr. Der eigene Weg ist `scripts/mitschnitt-deploy.sh` (bauen, mit des Betreibers Developer-ID signieren, installieren). Die Schritte unten, die diese Workflows nennen, sind Upstream-Historie und laufen hier nicht.

Heavy, cross-platform verification is deliberately **not** run on every PR. It is
batched into a short audit performed just before cutting a release. This keeps
per-PR feedback fast (so the Bugbot → fix → push loop stays cheap) while the real
"moment of truth" happens once, on purpose.

## CI model

- **Per PR (fast lane, Linux only):** lint, format, typecheck, unit/integration
  tests, and Linux `cargo check`/`cargo test`. Deduplicated via
  `concurrency: cancel-in-progress`, so rapid pushes cancel superseded runs.
- **On merge to `main` + nightly (`schedule`):** the full desktop matrix
  (macOS, Windows, Linux arm64, Swift) and the mobile native builds
  (iOS, watchOS, Android). Nightly catches platform breakage within a day and
  attributes it to a small window — keeping the release audit a clean diff review
  rather than a regression hunt.
- **Release (this audit):** full builds + signing + real-hardware QA.

## Audit checklist

Run these before publishing a stable desktop release.

1. **Read the cumulative diff since the last version.**
   `git diff <last-stable-tag>..main -- apps/desktop/src-tauri plugins crates apps/desktop/src`
   (see the `diff` task in `Taskfile.yaml`). Polish from first principles:
   simplify, delete dead code, reconcile inconsistencies introduced across PRs.

2. **Confirm the suites are green** on the release candidate -- im Fork
   lokal, nicht in der Cloud: `vitest run` und `tsc --noEmit` in
   `apps/desktop`, `cargo test` fuer die beruehrten Crates. Upstream lief
   hier `desktop_ci`/`mobile_ci`/`pro_api_e2e`; von den dreien existiert im
   Fork nur `desktop_ci.yaml`, und nichts wird gepusht.

3. **Build + sign** -- im Fork: `scripts/mitschnitt-deploy.sh` (macOS,
   Developer-ID-signiert, lokal installiert). Upstream lief hier `desktop_cd`
   (`staging`, dann `stable`) fuer macOS/Windows/Linux; dieser Workflow ist
   im Fork geloescht.

4. **Real-hardware QA** -- im Fork: das nummerierte Testprotokoll auf
   der frisch installierten App (Aufnahme, Transkription, Zusammenfassung).
   `.agents/skills/qa-critical-ux` beschreibt den Upstream-Durchlauf; sein
   Skript `run-native-dev-qa.sh` ist stillgelegt (Exit 2), weil es gegen
   `desktop_cd.yaml` und die Endpunkte des Originals baute.

5. **Changelog** -- eine `<version>.md` unter `packages/changelog/content`
   nach dem Muster in `LIESMICH.md` dort. Die Upstream-Anleitung
   (`.agents/skills/new-changelog`, verlangte `desktop_cd.yaml` und doxxer)
   ist geloescht.

6. **Cut the release** -- im Fork ist das Schritt 3. Die Upstream-Anleitung
   (`.agents/skills/release-new-version`, Dispatch von `desktop_cd.yaml`) ist
   geloescht.

## Notes

- Anything that fails incidentally but is out of scope for the release gate is
  tracked in Linear, not patched into the candidate (see `qa-critical-ux`).
- macOS/iOS/watchOS/Windows verification requires real Apple/Windows machines;
  the Linux Cloud Agent covers authoring + Linux-native checks only.
