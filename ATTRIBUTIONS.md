# Third-Party Components in Mitschnitt

Mitschnitt started as a fork of [anarlog](https://github.com/fastrepl/anarlog).
The app contains work by many other people. This file names them and the
conditions under which they may be shipped with it.

This file is included in the app bundle (`Contents/Resources/licenses/`), together
with the license texts it references. Anyone who distributes the app distributes this file
along with it.

**Short version, if you're short on time:** the base is MIT and unproblematic. There are
two things you should know before turning this into a product. First, the language models
loaded at runtime carry their own conditions (Llama and Gemma are not OSI licenses).
Second, exactly one model is missing a license statement: the Parakeet batch model, the
one that actually does the transcription. Both are covered in Section 4.

---

## 1. The Base

| | |
| --- | --- |
| Project | anarlog (formerly Hyprnote) |
| Source | https://github.com/fastrepl/anarlog |
| Rights holder | Fastrepl, Inc. |
| License | MIT |
| License text | [`LICENSE`](LICENSE), in the bundle as `licenses/LICENSE-anarlog-MIT.txt` |

The MIT license requires that the copyright notice and license text be included with
every copy. That's exactly why `LICENSE` is included in the app bundle.

Mitschnitt is an independent fork and is not affiliated with Fastrepl, Inc.

## 2. Fonts

| Font | Rights holder | License | Where |
| --- | --- | --- | --- |
| Pretendard | Kil Hyung-jin (incorporates parts of Adobe Source and Inter) | SIL Open Font License 1.1 | `crates/export-core/fonts/`, compiled into the binary via `include_bytes!` (PDF export). A second, currently unreferenced copy sits in `plugins/export/fonts/`. |
| Cabin Sketch | Impallari Type | SIL Open Font License 1.1 | Windows window decoration, `plugins/windows/swift-lib/src/Resources/` |

Both license texts sit alongside the font files and in the app bundle
(`licenses/Pretendard-OFL.txt`, `CabinSketch-OFL.txt`).

| Font | Rights holder | License | Where |
| --- | --- | --- | --- |
| New Computer Modern | GUST e-foundry | GUST Font License (LPPL-like) | comes in via the typst dependency, not as its own file in the repo |

## 3. Models Built into the App

These ONNX files are compiled into the binary via `include_bytes!`, so they're included in
every copy of the app.

| Model | Purpose | Source | License |
| --- | --- | --- | --- |
| pyannote Segmentation | Speaker segmentation | pyannote.audio | MIT, [model card](https://huggingface.co/pyannote/segmentation-3.0) |
| pyannote/WeSpeaker Embedding | Speaker voiceprint | pyannote.audio family | MIT, [model card](https://huggingface.co/pyannote/wespeaker-voxceleb-resnet34-LM) |
| Silero VAD v6.2 | Speech/pause detection | https://github.com/snakers4/silero-vad | MIT |
| DTLN | Noise suppression (`crates/denoise`) | https://github.com/breizhn/DTLN | MIT |
| DTLN-aec | Echo cancellation (`crates/aec`) | https://github.com/breizhn/DTLN-aec | MIT |

**Honest status on DTLN and DTLN-aec:** the attribution is very likely correct but not
proven. The ONNX files sit in the repo with no provenance note; the architecture and
naming match DTLN, but there's no accompanying evidence. The same applies to the
embedding model in `crates/embedding`, whose code explicitly follows pyannote.audio's
WeSpeaker implementation without naming the actual weights used. Anyone publishing the
app should clarify this beforehand.

## 3b. Language Data Built into the App

| File | Purpose | Source | License |
| --- | --- | --- | --- |
| `crates/vocabulary/wortliste-de.txt` | 38,424 ordinary German word forms. The guard that stops the dictionary-suggestion feature from swapping two everyday words for each other. | Derived from [UD German-GSD](https://github.com/UniversalDependencies/UD_German-GSD) | CC BY-SA 4.0 |

This file is compiled into the binary via `include_str!`, so it's included in every copy
of the app.

**How it's built:** from the treebank's word forms and lemmas, excluding anything tagged
there as a proper noun (`PROPN`), punctuation, number, or symbol. A word is included if it
occurs more often as an ordinary word than as a proper noun. Lowercased, umlauts expanded
to `ae`/`oe`/`ue`, `ß` to `ss`.

**Conditions of the source:** the treebank is licensed under Creative Commons
Attribution-ShareAlike 4.0 International
(<https://creativecommons.org/licenses/by-sa/4.0/>). The derived word list is therefore
under the same license. The rest of the app's source code is unaffected by this: the list
is a bundled data file, not part of the program text.

**Not used, and why** (assessed on September 4, 2026, because the choice of source
determines how useful the whole dictionary-suggestion feature is):

- Hunspell `de_DE_frami`: GPLv2/GPLv3, so it's off-limits for this app. It also contains
  first names and place names, which would have silently killed real name suggestions.
- Frequency lists from subtitles: contain surnames from the corpus, the same problem in a
  stronger form.
- UD German-HDT: the annotation scheme is under CC BY-SA 4.0, but the underlying **text
  may only be redistributed for academic purposes**. That's not viable for an app that is
  meant to be distributed.

## 4. Models Loaded at Runtime

These models are **not** part of the app. They're only downloaded once you select them in
the settings, and after that they live in your user folder. Even so, their conditions
still apply to you as a user.

| Model | License | Note |
| --- | --- | --- |
| Whisper (ggml, via whisper.cpp) | MIT | Loaded directly from Hugging Face (`ggerganov/whisper.cpp`). |
| NVIDIA Parakeet TDT v3, Batch (`aufklarer/Parakeet-TDT-v3-CoreML-INT8-30s`) | open, see below | The default model for transcribing finished recordings. CoreML conversion of [nvidia/parakeet-tdt-0.6b-v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) (CC-BY-4.0). |
| NVIDIA Parakeet EOU 120M, Live (`aufklarer/Parakeet-EOU-120M-CoreML-INT8`) | NVIDIA Open Model License | The model for live captions. Based on [nvidia/parakeet_realtime_eou_120m-v1](https://huggingface.co/nvidia/parakeet_realtime_eou_120m-v1). |
| pyannote Community-1, speaker diarization (`aufklarer/Pyannote-Community-1-CoreML`) | CC-BY-4.0, with `LICENSE` in the repo | Loaded together with the batch model to separate speakers. Based on [pyannote/speaker-diarization-community-1](https://huggingface.co/pyannote/speaker-diarization-community-1). |
| Omnilingual ASR CTC 300M (`aufklarer/Omnilingual-ASR-CTC-300M-CoreML-INT8-10s`) | Apache-2.0 | Present in the code, not offered in the selection. Based on `facebook/omniASR-CTC-300M`. |
| Llama 3.2 3B Instruct | Llama 3.2 Community License | **Not an OSI license.** Has its own conditions, including attribution and usage requirements. |
| Gemma 3 4B IT | Gemma Terms of Use | **Not an OSI license.** Has its own conditions, including usage restrictions. |

**Note on where things come from:** Llama and Gemma are loaded directly from Hugging
Face, and Whisper has been too since August 31, 2026. `hypr-llm-sm` (the upstream
project's language model) has not been offered since September 2, 2026: it could only be
loaded bit-for-bit from an S3 bucket owned by the upstream project
(`hyprnote.s3.us-east-1.amazonaws.com`). That's not a license problem, but it is a pipeline
that nobody owes us. The version on Hugging Face is not the same file (different
checksum), and the checksum is not being adjusted just to make it match. Details are in
the header of `crates/local-model/src/lib.rs`.

**The Parakeet models no longer come from Argmax.** Until September 1, 2026, the app
loaded three models from `argmaxinc/parakeetkit-pro` and `argmaxinc/whisperkit-pro`. Their
`LICENSE_NOTICE.txt` states literally: *“Argmax proprietary and confidential. Under NDA.
… Unauthorized access, copying, use, distribution, and or commercialization … is strictly
prohibited.”* Those repos are access-restricted on Hugging Face and licensed only for the
Argmax Pro SDK. The upstream project repackaged them and served them from its own S3
bucket. This entire path has been removed: not replaced, simply cut.

**The one open flank, named honestly:** the batch model
`aufklarer/Parakeet-TDT-v3-CoreML-INT8-30s` has **no license statement and no README** on
Hugging Face (checked September 1, 2026, **rechecked September 4, 2026: unchanged, the
page still says “No model card”**). That's a missing model card, not a prohibition: the
two sibling versions of the same conversion, `aufklarer/Parakeet-TDT-v3-CoreML-INT8` and
`…-iOS-5s`, both carry CC-BY-4.0 and describe the same architecture (FastConformer, 24
layers, 1024 hidden: exactly the values in the 30s repo's `config.json`), and the original
NVIDIA model is also licensed under CC-BY-4.0. **This should be clarified in writing before
any release.**

A switch to a source with a clearly stated license was considered and rejected:
`FluidInference/parakeet-tdt-0.6b-v3-coreml` (CC-BY-4.0) ships a differently cut compute
graph (15-second windows instead of 30, different input names, different file structure)
and doesn't fit the loader in `soniqo/speech-swift`. The assessment is documented in the
header of `crates/transcribe-soniqo/src/model.rs`.

Incidentally, `aufklarer` is not some third-party mirror; it's the model home of the
library that loads them: `soniqo/speech-swift` (Apache-2.0). The download runs in their
Swift code, not ours.

## 5. Third-Party Trademarks and Logos

In the model and provider selection, and in the integrations, the app displays logos and
word marks of these providers:

OpenAI, Meta, NVIDIA, Qwen (Alibaba), Deepgram, Gladia, Cartesia, Rev.ai, Speechmatics,
Soniox, Unsloth, AssemblyAI, pyannote, Obsidian, Slack, Zoom, Microsoft Teams, Google Meet,
Webex, Linear, GitHub, Apple.

These are trademarks of their respective owners. They are used solely to identify which
service or model is meant. There is no affiliation with these companies, and none of them
endorses, reviews, or approves Mitschnitt.

## 6. Sounds

The onboarding jingle `plugins/sfx/sounds/bgm.mp3` (from the upstream project, apparently
generated there with Suno based on the available evidence, its rights status not
determinable from the outside) was completely removed on September 22, 2026.

Two short notification sounds remain: `plugins/sfx/sounds/start_recording.ogg` and
`plugins/sfx/sounds/stop_recording.ogg`. Source: upstream anarlog, no license statement of
their own.

## 7. Additional Dependencies

**A name match that isn't one:** `crates/transcribe-soniqo/swift-lib` also pulls in
[WhisperKit](https://github.com/argmaxinc/WhisperKit) via `soniqo/speech-swift`, a package
from Argmax. This is **not** the proprietary path that was removed on September 1, 2026:
WhisperKit itself is licensed under **MIT** (`Copyright (c) 2024 argmax, inc.`), it is the
library rather than the Pro SDK's models, and speech-swift carries it along explicitly
only for benchmark comparison. So anyone finding `argmax` in `Package.resolved` has not
found a leftover of that removal.

Rust crates and npm packages carry their licenses in `Cargo.lock` and `pnpm-lock.yaml`.
The full, machine-generated list is in
[`apps/desktop/src/about/dritte-lizenzen.json`](apps/desktop/src/about/dritte-lizenzen.json)
(`node scripts/lizenzen-sammeln.mjs`, measured September 22, 2026: 1,255 third-party Rust
packages, 746 third-party npm packages). Most are MIT/Apache-2.0/BSD/ISC and require
nothing beyond attribution. What goes beyond that is listed individually below.

## 7b. Rust and npm Packages with Conditions Beyond Attribution

**MPL-2.0 (Rust, 24 packages):** `colored`, `cssparser`/`cssparser-macros`, `dtoa-short`,
`option-ext`, `progenitor`/`progenitor-client`/`progenitor-impl`/`progenitor-macro`,
`selectors`, `smartstring` (MPL-2.0+), plus the entire
[Symphonia](https://github.com/pdeljanov/Symphonia) family (`symphonia`,
`symphonia-core`, `symphonia-bundle-flac`, `symphonia-bundle-mp3`, `symphonia-codec-aac`,
`symphonia-codec-adpcm`, `symphonia-codec-alac`, `symphonia-codec-pcm`,
`symphonia-codec-vorbis`, `symphonia-format-caf`, `symphonia-format-isomp4`,
`symphonia-format-mkv`, `symphonia-format-ogg`, `symphonia-format-riff`,
`symphonia-metadata`, `symphonia-utils-xiph`) for audio decoding/encoding. MPL is
file-based copyleft: changes to the MPL files themselves would have to be disclosed,
libraries included unmodified do not. None of these have been modified here.
`htmlescape` additionally carries MPL-2.0 as one of three alternative licenses
(`Apache-2.0 / MIT / MPL-2.0`).

**CDLA-Permissive-2.0 (Rust, 3 packages):** `webpki-root-certs`, `webpki-roots` (two
versions, 0.26.11 and 1.0.7): certificate data from Mozilla's root store, packaged by
[rustls](https://github.com/rustls/webpki-roots). Permissive, but it requires attribution
of the data source, not just of the code.

**Dual licenses with a non-OSI half (Rust, 2 packages):** `ryu` and `whoami` offer
`Apache-2.0 OR BSL-1.0` (and, for one of them, additionally `OR MIT`): both are used under
Apache-2.0, BSL-1.0 (Boost) is the alternative, not a requirement.

**CC-BY-4.0 (npm, 1 package):** `caniuse-lite` (Browserslist database): data attribution,
no code copyleft.

**Dual licenses with a permissive choice (npm, 2 packages):** `dompurify`
(`MPL-2.0 OR Apache-2.0`, used here under Apache-2.0) and `json-schema`
(`AFL-2.1 OR BSD-3-Clause`, used here under BSD-3-Clause).

**LGPL-3.0 (npm), removed (September 22, 2026):** [`pos`](https://github.com/dariusk/pos-js)
0.4.2 (part-of-speech tagger) was listed here until September 22, 2026 as an actually-used
package, not merely an installed one, pulled in via `retext-pos` in
`apps/desktop/src/stt/useKeywords.ts` (`.use(retextPos)`, keyword extraction from
transcripts). `retext-pos` and `pos` have been replaced:
`apps/desktop/src/stt/keyword-importance.ts` (built in-house, no third-party dependency)
now handles the tagging via a stopword list instead of an English Penn Treebank tagger.
Measured against German text, `pos` never delivered a genuine part-of-speech analysis
anyway, since practically every German word fell back to the lexicon's default value
(rationale in the file header of `keyword-importance.ts`). Neither `pos` nor `retext-pos`
appear any longer in `pnpm-lock.yaml` or `dritte-lizenzen.json` (`pnpm why pos` returns
nothing).

**Without a license statement (4 packages, as measured):**
- `gbnf-validator` 0.1.0 (Rust, a git dependency taken directly from
  `github.com/fastrepl/gbnf-validator`): **does not end up in the shipped program.**
  `crates/gbnf` (the only consumer of `gbnf-validator` in the workspace) has no dependent
  of its own: neither another crate nor `apps/desktop/src-tauri` references `anlg-gbnf`. A
  dead workspace member, not a distribution problem.
- `@splinetool/runtime` 0.9.526 (npm): no license field in `package.json`, no `LICENSE` in
  the package, no public repo at `github.com/splinetool/runtime` (404, as of
  September 22, 2026); presumably proprietary, since Spline is a commercial product.
  **After a source-code check, it is not in the frontend bundle:** the package comes in
  only as a `dependencies` entry of `@lobehub/ui`; `apps/desktop/src` never imports from
  `@lobehub/ui`, only from `@lobehub/icons` (whose own `package.json` lists `@lobehub/ui`
  only as an empty `peerDependency`, with no Spline/Giscus connection). Not 100%
  excludable without an actual build, but the import chain argues clearly against it being
  shipped.
- `@giscus/react` 3.1.0 (npm): the same finding and the same non-use as
  `@splinetool/runtime` (identical path via `@lobehub/ui`).
- `khroma` 2.1.0 (npm): **this is a gap in `package.json`, not a missing license:** the
  package includes its own `license` file, which states MIT
  (`Copyright (c) 2019-present Fabio Spampinato, Andrew Maney`). Used via `mermaid`.

**LAME is an exception to this section.** It is covered in Section 8, because it is the
only dependency in the tree that does not carry a permissive license but one with a
condition on how it is built.

## 8. libmp3lame (LAME): The Only Copyleft Dependency

| | |
| --- | --- |
| Library | LAME 3.100 (`libmp3lame`) |
| Purpose | Saving recordings as MP3 (`crates/mp3`, used by `audio-norm`, `listener-core`, `listener2-core`) |
| Rights holder | Mark Taylor and the LAME project |
| License | **GNU LGPL, version 2 or, at your option, a later version** (`include/lame.h`: "version 2 of the License, or (at your option) any later version") |
| Rust binding | `mp3lame-sys 0.1.11`, **LGPL version 3** |
| Source | `vendor/mp3lame-sys/lame-3.100/`, unmodified; also <https://lame.sourceforge.io/> |
| License texts | `licenses/LICENSE-LGPL-2.0.txt`, `LICENSE-LGPL-3.0.txt`, `LICENSE-GPL-3.0.txt`, in the bundle at `Contents/Resources/licenses/` |
| Note for recipients | `licenses/LIESMICH-libmp3lame.md`, also in the bundle |

**What the condition consists of.** The LGPL permits building the library into a program
with its own license, even a closed one. In Section 4, it makes that conditional on the
recipient being able to **replace** the library with their own, modified version. Dynamic
linking is explicitly sufficient for this.

**What was wrong here until September 22, 2026.** `mp3lame-sys` builds LAME statically
(`--disable-shared`, `cargo:rustc-link-lib=static=mp3lame`). Measured on the shipped
bundle:

    nm -U Mitschnitt.app/Contents/MacOS/mitschnitt | grep -c " _lame_"   ->  67

67 defined LAME symbols in the main program. Replacement was impossible, and the license
texts were not included. Distribution in this form would not have been permissible.

**What applies now.** On macOS, LAME is built as its own file and sits in the bundle as
`Contents/Frameworks/libmp3lame.0.dylib`; the main program loads it via
`@rpath/libmp3lame.0.dylib`. The same command now outputs `0`. How to replace it is
described in `licenses/LIESMICH-libmp3lame.md`. The guard `scripts/lgpl-gate.sh` checks
this against the built bundle, so a package update cannot silently revert this state.

**What was changed in the library's source code: nothing.** Only how it is built was
changed (static to dynamic; the mpg123 decoder is compiled in as well, because LAME's own
export list requires it). The rationale, with the actual error messages observed, is in
the header of `vendor/mp3lame-sys/build.rs`.

**Decided (September 22, 2026, commit `c12de21af2`).** With Hardened Runtime, macOS only
loads libraries signed with the same team ID as the app. That would mean a recipient could
not load their own version, and the right to replace the library would remain formally
granted but practically blocked. There is exactly one entitlement that addresses this:
`com.apple.security.cs.disable-library-validation`, **set** in `Entitlements.notar.plist`
as of this commit. It measurably weakens the runtime (the app then also loads
third-party-signed libraries from its own bundle), and was therefore a deliberate
decision, not a side effect of a rebuild. Notarized distribution is permitted as a result.

**An ad hoc re-signature by the recipient invalidates the notarization.** The entitlement
above allows the dylib to be swapped for a DIFFERENTLY signed version without macOS
rejecting the app because of it. It does not change the fact that re-signing the ENTIRE
bundle (for example via `codesign --force --deep --sign -` after the swap, as described in
`licenses/LIESMICH-libmp3lame.md`) invalidates the attached notarization ticket. Gatekeeper
will then prompt again, exactly as with an unnotarized app: expected and fine for testing
on your own machine, but no longer suitable for distribution to third parties.

**Linux and Windows have not been converted.** There, `mp3lame-sys` still builds
statically. As long as nothing is distributed from those platforms, this has no
consequences; as soon as it is, this section applies there just the same, and the
conversion is still pending.

---

*As of: September 22, 2026 (Section 8 was reassessed and written on this day; the models
section on September 1). If you add a dependency that carries its own conditions, it
belongs in here before the next copy leaves the building.*
