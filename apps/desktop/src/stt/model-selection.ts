type ModelEntry = {
  id: string;
  isDownloaded?: boolean;
};

type PreferredProviderModelOptions = {
  allowSavedModelWithoutChoices?: boolean;
  keepUnavailableSavedModel?: boolean;
};

const DEFAULT_EXTERNAL_STT_MODELS: Record<string, string> = {
  local_file: "local-file",
  deepgram: "nova-3-general",
  assemblyai: "universal-3-5-pro",
  openai: "gpt-live-transcribe",
  openrouter: "openai/gpt-transcribe",
  cartesia: "ink-2",
  cloudflare_workers_ai: "nova-3",
  gladia: "solaria-1",
  soniox: "stt-rt-v5",
  elevenlabs: "scribe_v2",
  mistral: "voxtral-mini-2602",
  pyannote: "parakeet-tdt-0.6b-v3",
  aquavoice: "avalon-v1-en",
  cohere: "cohere-transcribe-03-2026",
  fireworks: "whisper-v3-turbo",
  groq: "whisper-large-v3-turbo",
  xai: "xai-stt",
  together: "openai/whisper-large-v3",
  speechmatics: "enhanced",
  azure_speech: "fast-transcription",
  google_cloud: "latest_long",
  google_generative_ai: "gemini-3.5-transcribe-live",
  aws_transcribe: "amazon-transcribe",
  revai: "machine",
};

export function getDefaultSttModel(provider?: string | null) {
  return provider ? DEFAULT_EXTERNAL_STT_MODELS[provider] : undefined;
}

export function normalizeStoredSttModel(
  provider: string | undefined,
  model: string | undefined,
) {
  if (provider === "assemblyai" && model === "universal") {
    return "universal-3-5-pro";
  }

  if (provider === "soniox") {
    const alias = model?.match(/^stt-(?:async-|rt-)?v([3-5])$/);
    if (alias) {
      const version = alias[1] === "3" ? "4" : alias[1];
      return `stt-rt-v${version}`;
    }
  }

  return model;
}

/**
 * Die drei Argmax-Modelle, die es in diesem Fork nicht mehr gibt, und wohin eine
 * gespeicherte Einstellung stattdessen zeigt.
 *
 * Warum das noetig ist: `LocalModel` ist auf der Rust-Seite ein `#[serde(untagged)]`-Typ.
 * Bleibt `"am-parakeet-v3"` in der Datenbank stehen, scheitert nach dem Wegfall der
 * Variante JEDE local-stt-Kommando-Deserialisierung mit "data did not match any variant"
 * -- die Oberflaeche kaeme dann gar nicht mehr an ihre Modellliste.
 *
 * Die Ziele sind keine Notloesung, sondern die jeweiligen Nachfolger derselben Modelle:
 * Parakeet TDT v3 laeuft im Fork ueber soniqo, Whisper large-v3 ueber den lokalen
 * whisper.cpp-Dienst. Beide muessen einmal geladen werden -- die Argmax-Gewichte konnten
 * das ohnehin nicht mehr, ihr Sidecar ist weg.
 */
const RETIRED_ARGMAX_STT_MODELS: Record<
  string,
  { provider: string; model: string }
> = {
  "am-parakeet-v2": { provider: "soniqo", model: "soniqo-parakeet-batch" },
  "am-parakeet-v3": { provider: "soniqo", model: "soniqo-parakeet-batch" },
  "am-whisper-large-v3": {
    provider: "whispercpp",
    model: "QuantizedLargeTurbo",
  },
};

/**
 * Die Kennungen der Gegenstelle des Originals ("anarlog", davor "hyprnote").
 * Was unter ihnen gespeichert ist und kein lokales Modell benennt -- vor allem
 * das Cloud-Modell "cloud" -- kann in diesem Fork nie wieder verbinden.
 * `isConfiguredSttModel` hielt so eine Wahl trotzdem fuer eingerichtet, es
 * gab also nicht einmal den Hinweis-Banner (Kimi/Grok-Review 02.09.2026, F7).
 *
 * Beide Kennungen werden in jedem Zweig gleich behandelt (Opus 4, Review
 * 02.09.2026): vorher griffen die Umzuege fuer soniqo-* und apple-speech nur
 * fuer "anarlog", und "hyprnote" + "soniqo-parakeet-streaming" verlor die
 * Live-Wahl an den Batch-Standard.
 */
export const LEGACY_HOSTED_STT_PROVIDER_IDS: ReadonlySet<string> = new Set([
  "anarlog",
  "hyprnote",
]);

/**
 * Wohin so eine Wahl stattdessen zeigt: derselbe lokale Standard, auf den die
 * Batch-Transkription ohne Verbindung ohnehin zurueckfaellt
 * (`LOCAL_SONIQO_BATCH_TARGET` in useRunBatch.ts). die eigene Installation
 * steht seit dem Umstieg genau dort (gemessen 02.09.2026 an der Kopie
 * `_backup-2026-09-01-2246/app.db`).
 */
export const LOCAL_STT_DEFAULT_SELECTION = {
  provider: "soniqo",
  model: "soniqo-parakeet-batch",
} as const;

export function normalizeStoredSttSelection(
  provider: string | undefined,
  model: string | undefined,
) {
  const normalizedModel = normalizeStoredSttModel(provider, model);

  if (normalizedModel && normalizedModel in RETIRED_ARGMAX_STT_MODELS) {
    return RETIRED_ARGMAX_STT_MODELS[normalizedModel];
  }

  if (!provider || !LEGACY_HOSTED_STT_PROVIDER_IDS.has(provider)) {
    return { provider, model: normalizedModel };
  }

  // Ein lokales Modell unter der Alt-Kennung behaelt sein Modell und zieht
  // nur zum Anbieter um, der es heute traegt.
  if (normalizedModel?.startsWith("soniqo-")) {
    return { provider: "soniqo", model: normalizedModel };
  }
  if (normalizedModel === "apple-speech") {
    return { provider: "apple_speech", model: normalizedModel };
  }
  if (normalizedModel?.startsWith("Quantized")) {
    return { provider: "whispercpp", model: normalizedModel };
  }
  return { ...LOCAL_STT_DEFAULT_SELECTION };
}

const normalizeSavedModel = (
  savedModel: string | undefined,
  models: ModelEntry[],
) => {
  if (savedModel === "universal") {
    if (models.some((model) => model.id === "universal-3-5-pro")) {
      return "universal-3-5-pro";
    }

    if (models.some((model) => model.id === "universal-3-pro")) {
      return "universal-3-pro";
    }

    if (models.some((model) => model.id === "u3-rt-pro")) {
      return "u3-rt-pro";
    }
  }

  const sonioxRealtimeAlias = savedModel?.match(
    /^stt-(?:async-|rt-)?v([3-5])$/,
  );
  if (sonioxRealtimeAlias) {
    const version =
      sonioxRealtimeAlias[1] === "3" ? "4" : sonioxRealtimeAlias[1];
    const realtimeModel = `stt-rt-v${version}`;
    if (models.some((model) => model.id === realtimeModel)) {
      return realtimeModel;
    }
  }

  return savedModel;
};

export function getPreferredProviderModel(
  savedModel: string | undefined,
  models: ModelEntry[],
  options?: PreferredProviderModelOptions,
) {
  const normalizedSavedModel = normalizeSavedModel(savedModel, models);
  const selectableModels = models.filter((model) => model.isDownloaded ?? true);

  if (
    options?.keepUnavailableSavedModel &&
    normalizedSavedModel &&
    models.some((model) => model.id === normalizedSavedModel)
  ) {
    return normalizedSavedModel;
  }

  if (
    normalizedSavedModel &&
    selectableModels.some((model) => model.id === normalizedSavedModel)
  ) {
    return normalizedSavedModel;
  }

  if (selectableModels.length > 0) {
    return selectableModels[0].id;
  }

  if (options?.allowSavedModelWithoutChoices) {
    return normalizedSavedModel ?? "";
  }

  return "";
}
