# Changelog

All notable changes to Mitschnitt. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [0.1.6] - 2026-09-22

The first version with a built-in updater. Install this one, and every later version arrives by itself.

### Added
- **Automatic updates** from this repository's releases. Found at launch: installed and restarted once. Found later: a notice with a button. Never during a recording. Every update is checked against its signature before it is installed.
- Mitschnitt's own menu bar icons.

### Changed
- The menu bar shows only the icon, no meeting title.
- The MP3 encoder (LAME) is now linked dynamically, so it can be swapped as its LGPL license intends. Instructions ship with the license texts.
- The part-of-speech library under LGPL was replaced by a built-in stop-word list.

### Removed
- The onboarding sound.

## 0.1.5 - 2026-09-22

### Added
- **Resume after an accidental stop.** A "Resume" button stays available after you stop, even while the transcript is being written. One click keeps recording into the same meeting. If a meeting ends up with more than one recording, Mitschnitt offers to transcribe it again in one go.
- A visible **Transcribe** button in the transcript tab.

### Changed
- The app is signed and notarized by Apple, so macOS opens it without a warning.
- First launch downloads the transcription model before the app opens, so the first recording always has a model to work with.
- The setup says plainly that summaries need a provider (Apple Intelligence, Ollama/LM Studio, or your own API key).

## 0.1.4 - 2026-09-22

### Changed
- First launch no longer gets stuck on the permissions step.
- Provider lists show local models and the most common providers first, the rest behind "More".
- The Accessibility permission explains what depends on it and where to grant it.
- The speaker count takes into account which channel each person was recorded on.

## 0.1.3 - 2026-09-22

### Changed
- Speaker labels no longer run out after a fixed number of speakers.
- Short fragments are merged into the surrounding speaker, and speaker assignments are smoothed, so transcripts read as conversation rather than ping-pong.

## 0.1.2 - 2026-09-21

### Added
- **Speaker separation for long recordings.** Long meetings are processed in parts instead of being skipped, faster and with far less memory.

### Fixed
- A false "channel is silent" warning.

## 0.1.1 - 2026-09-11

### Added
- **Import from anarlog and Hyprnote.** Finds the old data by itself, brings in meetings, transcripts, notes and recordings, and leaves the source untouched.
- The `mitschnitt` command-line tool and its **MCP server** for AI agents. Agents can read meetings; writes are only ever proposals you approve in the app.

### Fixed
- The command-line tool now always finds the app's database.

## 0.1.0 - 2026-09-04

The first release of Mitschnitt.

- Local recording of microphone and system audio, no bot in the call.
- Local transcription with Parakeet or Whisper large-v3-turbo, with real word timings.
- On-device speaker separation and speaker names from your calendar.
- A vocabulary that corrects names it keeps mishearing.
- Search inside transcripts.
- A plain Markdown and audio folder for every meeting.
- Settings export and import, and an optional retention period for old recordings.
- No account, no cloud sync, no telemetry.

[0.1.6]: https://github.com/simeonzickert/mitschnitt/releases/tag/v0.1.6
