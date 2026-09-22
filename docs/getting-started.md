# Getting started with Mitschnitt

Mitschnitt records your meetings and writes them down. Everything runs on your own Mac.

You need a Mac with Apple Silicon (M1 or newer) and **macOS 15 or later**. The app does not start on older versions.

([Deutsche Fassung](getting-started.de.md))

---

## 1. Open the app for the first time

1. Download `Mitschnitt-<version>-notarisiert.zip` from the [latest release](https://github.com/simeonzickert/mitschnitt/releases/latest) and unzip it.
2. Drag **Mitschnitt** into your **Applications** folder.
3. Double-click the app.

The app is notarized by Apple, so macOS opens it without a warning. Later versions install themselves (see section 5).

---

## 2. What the app asks for on first launch

A short setup walks you through a few permissions. Each one is requested only when you click it; nothing happens on its own.

1. **Microphone**, so your own voice is recorded.
2. **System audio**, so the other people in the call are recorded. Without it, you only hear yourself.
3. **Accessibility**, so the app notices when a call starts. A small assistant guides you through System Settings.
4. **Calendar**, optional. Recordings then get the title and participants of the matching event automatically. You can skip this step.

Afterwards the app creates a sample meeting called "Welcome to Mitschnitt" so you can see what a result looks like.

---

## 3. The transcription model

On first launch the app downloads its transcription model (Parakeet, about 660 MB). The button to open the app appears once it's there. Models are not bundled because they are too large.

More models are under **Settings → Transcription**: pick one, click **Download**.

| Model | Size | What for |
| --- | --- | --- |
| Parakeet (batch) | about 660 MB | The transcript after the meeting. The usual path. |
| Parakeet (streaming) | about 125 MB | Live captions during the meeting. |
| Whisper large-v3-turbo | about 830 MB | Slower, but better with names and punctuation. |

Another 35 MB or so go to speaker separation, which is what puts the right speaker next to each line.

If a download fails, a notice appears at the top right. **If you dismiss that notice, the reason is gone** and the row simply returns to the download button. Just try again.

---

## 4. Summaries

Transcription runs entirely on your Mac. A **summary** needs a language model on top, and there are these ways to get one:

- **Apple Intelligence**, the only way without an account or extra software. Requires macOS 26 or later, a Mac that supports Apple Intelligence, and Apple Intelligence switched on in System Settings. Find it under **Settings → Intelligence**. It is marked as experimental and works best for shorter meetings.
- **Your own API key** for a provider such as OpenAI, Anthropic or Google, entered under **Settings → Intelligence**. Note: the meeting text is then sent to that provider.
- **Ollama or LM Studio**, if you already run one of them on your Mac.

If no model is available, the app shows a hint with a button to the right settings instead of a summary. Recording and transcription keep working regardless.

---

## 5. What the app does not do

- **It sends neither audio nor text anywhere** as long as you stay with local models. Recordings, transcripts and summaries live in your user folder.
- **No sign-in, no account, no subscription.** If anything asks you for an account, that's a bug. Please report it.
- **No crash reports and no usage statistics.** There is no service built in for them.

Three exceptions, to keep it honest:

- When you download a model (section 3), the app fetches the file from Hugging Face.
- At launch and then about every 30 minutes, the app asks GitHub (`github.com/simeonzickert/mitschnitt/releases`) whether there is a newer version. Only that request goes out, no recording and no text. If it finds a new version at launch, it downloads it, installs it and restarts once; if it finds one later, it shows a notice with a button. It never updates during a recording; that waits until after the meeting.
- If you add a provider with your own key under "Intelligence", the meeting text goes to that provider. That's your call.

---

## 6. Where the licenses are

The app builds on the work of many other people, and their terms ship with it.

- **In the app:** Settings → **About**. It lists the origin, the models with their licenses and the full list of components.
- **As files:** right-click Mitschnitt in Applications → **Show Package Contents** → `Contents/Resources/licenses/`.

---

## 7. When something doesn't work

[Open an issue](https://github.com/simeonzickert/mitschnitt/issues) or write to the address in [SECURITY.md](../SECURITY.md). These three things help most:

1. What you were doing when it happened.
2. What you expected, and what happened instead.
3. Your macOS version (Apple menu → About This Mac).

A screenshot almost always helps.
