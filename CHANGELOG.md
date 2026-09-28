# Changelog

All notable changes to Mitschnitt. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

Every version heading below is `## X.Y.Z (YYYY-MM-DD)`. `scripts/changelog-extract.sh` reads this file by that exact heading and by the `### What's new` / optional `### Note` subheadings underneath it to build the GitHub release notes and the in-app update notice — see that script's header comment for the contract. Before a release ships, its section here must exist and `### What's new` must not be empty, or `scripts/mitschnitt-release.sh` refuses to build.

## 0.1.8 (2026-09-28)

### Note

Coming from 0.1.6 or 0.1.7? Install this version once by hand from the DMG. Those versions download updates but cannot install them (fixed here). From 0.1.8 on, updates install themselves.

### What's new

- **Fixed: updates can be installed.** The updater compared the downloaded version against an empty app version and refused every update ("… is not newer than current"). It now reads the real app version.

All changes from 0.1.7 are included, see below.

## 0.1.7 (2026-09-28)

### What's new

**Speakers in cloud transcription**
- Cloud transcription now keeps your microphone and the other side apart: in a 1:1 call with headphones you and your counterpart are two speakers.
- OpenRouter models with word timings: **Gemini 3.5 Transcribe** (recommended), MAI Transcribe 2, Grok STT, Deepgram Nova-3, Parakeet, Whisper large-v3-turbo. Sentences stay together instead of being cut up word by word.
- Much faster with these models: a 35-minute call took about 3 minutes instead of 10.
- On Macs with 16 GB or more, calls with several people on the other side are split into separate speakers.
- `gpt-transcribe` has no word timings and is marked as not recommended for conversations.
- Summaries and chat no longer put names on quotes when speakers were not actually separated.
- Older cloud transcripts show a hint to re-transcribe them with speaker separation.
- Every full re-transcription asks before replacing your corrections.

**Participants dialog** (after recording without a calendar event)
- Suggestions can be clicked, clear *Continue* and *Skip*, Esc or clicking outside skips. Asked once per conversation.
- People with the same name are told apart by email, phone, role or a number.

**Interface**
- English and German throughout, including the menu bar, Dock menu and system dialogs (Settings → General → Main language).
- Provider lists grouped into **Local**, **Cloud** and **More**, for transcription and intelligence. Apple Speech is marked as low quality.
- New typeface: Geist.
- Import from any folder; the anarlog/Hyprnote entry only appears when data from that app is found.

**Distribution**
- Download as a DMG instead of a zip.

## 0.1.6 (2026-09-22)

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

## 0.1.5 (2026-09-22)

### Added
- **Resume after an accidental stop.** A "Resume" button stays available after you stop, even while the transcript is being written. One click keeps recording into the same meeting. If a meeting ends up with more than one recording, Mitschnitt offers to transcribe it again in one go.
- A visible **Transcribe** button in the transcript tab.

### Changed
- The app is signed and notarized by Apple, so macOS opens it without a warning.
- First launch downloads the transcription model before the app opens, so the first recording always has a model to work with.
- The setup says plainly that summaries need a provider (Apple Intelligence, Ollama/LM Studio, or your own API key).

## 0.1.4 (2026-09-22)

### Changed
- First launch no longer gets stuck on the permissions step.
- Provider lists show local models and the most common providers first, the rest behind "More".
- The Accessibility permission explains what depends on it and where to grant it.
- The speaker count takes into account which channel each person was recorded on.

## 0.1.3 (2026-09-22)

### Changed
- Speaker labels no longer run out after a fixed number of speakers.
- Short fragments are merged into the surrounding speaker, and speaker assignments are smoothed, so transcripts read as conversation rather than ping-pong.

## 0.1.2 (2026-09-21)

### Added
- **Speaker separation for long recordings.** Long meetings are processed in parts instead of being skipped, faster and with far less memory.

### Fixed
- A false "channel is silent" warning.

## 0.1.1 (2026-09-11)

### Added
- **Import from anarlog and Hyprnote.** Finds the old data by itself, brings in meetings, transcripts, notes and recordings, and leaves the source untouched.
- The `mitschnitt` command-line tool and its **MCP server** for AI agents. Agents can read meetings; writes are only ever proposals you approve in the app.

### Fixed
- The command-line tool now always finds the app's database.

## 0.1.0 (2026-09-04)

The first release of Mitschnitt.

- Local recording of microphone and system audio, no bot in the call.
- Local transcription with Parakeet or Whisper large-v3-turbo, with real word timings.
- On-device speaker separation and speaker names from your calendar.
- A vocabulary that corrects names it keeps mishearing.
- Search inside transcripts.
- A plain Markdown and audio folder for every meeting.
- Settings export and import, and an optional retention period for old recordings.
- No account, no cloud sync, no telemetry.
