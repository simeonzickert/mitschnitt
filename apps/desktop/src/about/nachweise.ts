import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";

// Die Nachweise, die eine Weitergabe der App zur BEDINGUNG hat.
//
// Diese Liste steht bewusst fest im Code und nicht in der erzeugten
// `dritte-lizenzen.json`: die erzeugte Datei kennt nur Rust- und npm-Pakete.
// Die Sprachmodelle werden zur Laufzeit geladen und tauchen in keinem
// Abhaengigkeitsbaum auf -- sie waeren also genau die Nachweise, die eine
// automatische Sammlung stillschweigend auslaesst.
//
// `zweck`/`lizenz` sind MessageDescriptors, keine rohen Strings: der About-
// Bildschirm rendert sie ueber `i18n._()`, sonst stuende hier deutscher
// Text, der bei Englisch-Einstellung trotzdem Deutsch bliebe (gefunden
// 26.09.2026 -- die begleitende Prosa in about/index.tsx lief laengst durch
// Lingui, nur diese Datentabelle nicht).
//
// Ausfuehrliche Fassung mit Herleitung und offenen Punkten: ATTRIBUTIONS.md im
// Wurzelverzeichnis, im gebauten Bundle unter
// `Contents/Resources/licenses/ATTRIBUTIONS.md`.

export type Nachweis = {
  name: string;
  zweck: MessageDescriptor;
  lizenz: MessageDescriptor;
  /** Namensnennung ist Bedingung der Weitergabe, nicht Hoeflichkeit. */
  namensnennungPflicht: boolean;
  url?: string;
};

/** Woraus die App entstanden ist. MIT verlangt die Weitergabe dieses Hinweises. */
export const HERKUNFT = {
  projekt: "anarlog",
  rechteinhaber: "Fastrepl, Inc.",
  lizenz: "MIT",
  url: "https://github.com/fastrepl/anarlog",
} as const;

/** Fest ins Programm kompiliert -- in jeder Kopie der App enthalten. */
export const MODELLE_MITGELIEFERT: Nachweis[] = [
  {
    name: "pyannote Segmentation 3.0",
    zweck: msg`Speaker segmentation`,
    lizenz: msg`MIT`,
    namensnennungPflicht: true,
    url: "https://huggingface.co/pyannote/segmentation-3.0",
  },
  {
    name: "pyannote / WeSpeaker Embedding",
    zweck: msg`Speaker voice fingerprint`,
    lizenz: msg`MIT`,
    namensnennungPflicht: true,
    url: "https://huggingface.co/pyannote/wespeaker-voxceleb-resnet34-LM",
  },
  {
    name: "Silero VAD v6.2",
    zweck: msg`Speech and pause detection`,
    lizenz: msg`MIT`,
    namensnennungPflicht: true,
    url: "https://github.com/snakers4/silero-vad",
  },
  {
    name: "DTLN",
    zweck: msg`Noise suppression`,
    lizenz: msg`MIT`,
    namensnennungPflicht: true,
    url: "https://github.com/breizhn/DTLN",
  },
  {
    name: "DTLN-aec",
    zweck: msg`Echo cancellation`,
    lizenz: msg`MIT`,
    namensnennungPflicht: true,
    url: "https://github.com/breizhn/DTLN-aec",
  },
];

/**
 * Erst auf Auswahl heruntergeladen, danach im Benutzerordner. Nicht Teil der
 * App -- die Bedingungen gelten trotzdem.
 */
export const MODELLE_GELADEN: Nachweis[] = [
  {
    name: "NVIDIA Parakeet TDT v3",
    zweck: msg`Transcription of finished recordings (default)`,
    lizenz: msg`CC-BY-4.0`,
    namensnennungPflicht: true,
    url: "https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3",
  },
  {
    name: "NVIDIA Parakeet EOU 120M",
    zweck: msg`Live captions`,
    lizenz: msg`NVIDIA Open Model License`,
    namensnennungPflicht: true,
    url: "https://huggingface.co/nvidia/parakeet_realtime_eou_120m-v1",
  },
  {
    name: "pyannote Community-1",
    zweck: msg`Speaker diarization`,
    lizenz: msg`CC-BY-4.0`,
    namensnennungPflicht: true,
    url: "https://huggingface.co/pyannote/speaker-diarization-community-1",
  },
  {
    name: "Whisper (ggml)",
    zweck: msg`Transcription, alternative`,
    lizenz: msg`MIT`,
    namensnennungPflicht: true,
    url: "https://huggingface.co/ggerganov/whisper.cpp",
  },
  {
    name: "Llama 3.2 3B Instruct",
    zweck: msg`Summaries`,
    lizenz: msg`Llama 3.2 Community License (not an OSI license)`,
    namensnennungPflicht: true,
    url: "https://huggingface.co/lmstudio-community/Llama-3.2-3B-Instruct-GGUF",
  },
  {
    name: "Gemma 3 4B IT",
    zweck: msg`Summaries, alternative`,
    lizenz: msg`Gemma Terms of Use (not an OSI license)`,
    namensnennungPflicht: true,
    url: "https://huggingface.co/unsloth/gemma-3-4b-it-GGUF",
  },
];

/**
 * Fremde Bibliotheken, die als eigene Datei im Bundle liegen.
 *
 * Bisher gibt es genau eine, und sie ist der Grund, warum es diese Liste gibt:
 * LAME steht unter der LGPL. Deren Paragraf 4 erlaubt den Einbau in ein
 * Programm mit eigener Lizenz nur, wenn der Empfaenger die Bibliothek
 * austauschen kann. Deshalb ist sie nicht ins Programm hineinkompiliert,
 * sondern liegt unter `Contents/Frameworks/`. Die Anleitung zum Austauschen
 * gehoert zur Bedingung und liegt im Bundle unter
 * `Resources/licenses/LIESMICH-libmp3lame.md`.
 */
export const BIBLIOTHEKEN: Nachweis[] = [
  {
    name: "LAME 3.100 (libmp3lame)",
    zweck: msg`Save recordings as MP3, replaceable under Contents/Frameworks`,
    lizenz: msg`GNU LGPL 2 or later (mp3lame-sys binding: LGPL 3)`,
    namensnennungPflicht: true,
    url: "https://lame.sourceforge.io/",
  },
];

/** Schriften, die im Programm oder im PDF-Export stecken. */
export const SCHRIFTEN: Nachweis[] = [
  {
    name: "Pretendard",
    zweck: msg`PDF export`,
    lizenz: msg`SIL Open Font License 1.1`,
    namensnennungPflicht: true,
  },
  {
    name: "Cabin Sketch",
    zweck: msg`Window decoration (Windows)`,
    lizenz: msg`SIL Open Font License 1.1`,
    namensnennungPflicht: true,
  },
  {
    name: "Geist",
    zweck: msg`App typeface (interface and headings)`,
    lizenz: msg`SIL Open Font License 1.1`,
    namensnennungPflicht: true,
    url: "https://github.com/vercel/geist-font",
  },
  {
    name: "Geist Mono",
    zweck: msg`App typeface (fixed-width text)`,
    lizenz: msg`SIL Open Font License 1.1`,
    namensnennungPflicht: true,
    url: "https://github.com/vercel/geist-font",
  },
];
