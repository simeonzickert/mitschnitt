// Die Nachweise, die eine Weitergabe der App zur BEDINGUNG hat.
//
// Diese Liste steht bewusst fest im Code und nicht in der erzeugten
// `dritte-lizenzen.json`: die erzeugte Datei kennt nur Rust- und npm-Pakete.
// Die Sprachmodelle werden zur Laufzeit geladen und tauchen in keinem
// Abhaengigkeitsbaum auf -- sie waeren also genau die Nachweise, die eine
// automatische Sammlung stillschweigend auslaesst.
//
// Ausfuehrliche Fassung mit Herleitung und offenen Punkten: ATTRIBUTIONS.md im
// Wurzelverzeichnis, im gebauten Bundle unter
// `Contents/Resources/licenses/ATTRIBUTIONS.md`.

export type Nachweis = {
  name: string;
  zweck: string;
  lizenz: string;
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
    zweck: "Sprecher-Segmentierung",
    lizenz: "MIT",
    namensnennungPflicht: true,
    url: "https://huggingface.co/pyannote/segmentation-3.0",
  },
  {
    name: "pyannote / WeSpeaker Embedding",
    zweck: "Sprecher-Stimmabdruck",
    lizenz: "MIT",
    namensnennungPflicht: true,
    url: "https://huggingface.co/pyannote/wespeaker-voxceleb-resnet34-LM",
  },
  {
    name: "Silero VAD v6.2",
    zweck: "Sprech- und Pausenerkennung",
    lizenz: "MIT",
    namensnennungPflicht: true,
    url: "https://github.com/snakers4/silero-vad",
  },
  {
    name: "DTLN",
    zweck: "Rauschunterdrückung",
    lizenz: "MIT",
    namensnennungPflicht: true,
    url: "https://github.com/breizhn/DTLN",
  },
  {
    name: "DTLN-aec",
    zweck: "Echo-Unterdrückung",
    lizenz: "MIT",
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
    zweck: "Transkription fertiger Aufnahmen (Standard)",
    lizenz: "CC-BY-4.0",
    namensnennungPflicht: true,
    url: "https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3",
  },
  {
    name: "NVIDIA Parakeet EOU 120M",
    zweck: "Live-Untertitel",
    lizenz: "NVIDIA Open Model License",
    namensnennungPflicht: true,
    url: "https://huggingface.co/nvidia/parakeet_realtime_eou_120m-v1",
  },
  {
    name: "pyannote Community-1",
    zweck: "Sprechertrennung",
    lizenz: "CC-BY-4.0",
    namensnennungPflicht: true,
    url: "https://huggingface.co/pyannote/speaker-diarization-community-1",
  },
  {
    name: "Whisper (ggml)",
    zweck: "Transkription, Alternative",
    lizenz: "MIT",
    namensnennungPflicht: true,
    url: "https://huggingface.co/ggerganov/whisper.cpp",
  },
  {
    name: "Llama 3.2 3B Instruct",
    zweck: "Zusammenfassungen",
    lizenz: "Llama 3.2 Community License (keine OSI-Lizenz)",
    namensnennungPflicht: true,
    url: "https://huggingface.co/lmstudio-community/Llama-3.2-3B-Instruct-GGUF",
  },
  {
    name: "Gemma 3 4B IT",
    zweck: "Zusammenfassungen, Alternative",
    lizenz: "Gemma Terms of Use (keine OSI-Lizenz)",
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
    zweck: "Aufnahmen als MP3 speichern, austauschbar in Contents/Frameworks",
    lizenz: "GNU LGPL 2 oder später (Anbindung mp3lame-sys: LGPL 3)",
    namensnennungPflicht: true,
    url: "https://lame.sourceforge.io/",
  },
];

/** Schriften, die im Programm oder im PDF-Export stecken. */
export const SCHRIFTEN: Nachweis[] = [
  {
    name: "Pretendard",
    zweck: "PDF-Export",
    lizenz: "SIL Open Font License 1.1",
    namensnennungPflicht: true,
  },
  {
    name: "Cabin Sketch",
    zweck: "Fensterdekoration (Windows)",
    lizenz: "SIL Open Font License 1.1",
    namensnennungPflicht: true,
  },
];
