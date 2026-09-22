import { disable, enable } from "@tauri-apps/plugin-autostart";
import { arch, platform } from "@tauri-apps/plugin-os";
import { useCallback } from "react";

import { commands as detectCommands } from "@anlg/plugin-detect";
import { commands as localSttCommands } from "@anlg/plugin-local-stt";
import { commands as templateCommands } from "@anlg/plugin-template";
import { commands as trayCommands } from "@anlg/plugin-tray";
import { commands as updaterCommands } from "@anlg/plugin-updater2";
import { commands as windowsCommands } from "@anlg/plugin-windows";

import { normalizeStoredLlmSelection } from "~/ai/llm-selection";
import { executeTransaction, liveQueryClient, useLiveQuery } from "~/db";
import { enqueueDatabaseWrite } from "~/db/write-queue";
import {
  DEFAULT_AUDIO_RETENTION,
  normalizeAudioRetention,
  UNASKED_INSTALL_AUDIO_RETENTION,
} from "~/services/audio-retention-policy";
import {
  LEGACY_MAIN_VALUES_ID,
  LEGACY_SETTINGS_ID,
} from "~/settings/legacy-snapshots";
import {
  SETTING_DEFINITIONS,
  type SettingKey,
  type SettingValue,
  type SettingValues,
} from "~/settings/schema";
import { isAppStoreBuild } from "~/shared/app-store";
import {
  isConfiguredSttModel,
  isOnDeviceSttModel,
  resolveServableSttSelection,
} from "~/stt/capabilities";
import {
  getDefaultSttModel,
  normalizeStoredSttModel,
} from "~/stt/model-selection";

type AppSettingRow = { id: string; value_json: string };

export type StoredSettingValues = {
  values: SettingValues;
  hasValues: Set<SettingKey>;
};

const EMPTY_STORED_SETTINGS: StoredSettingValues = {
  values: {},
  hasValues: new Set(),
};
const JSON_ARRAY_KEYS = new Set<SettingKey>([
  "spoken_languages",
  "personalization_dictionary_terms",
  "personalization_dictionary_dismissed",
  "ignored_platforms",
  "included_platforms",
]);
const LEGACY_SUMMARY_TEMPLATE_TOKEN = /\{\{\s*template\s*\}\}/g;
const LEGACY_DEFAULT_SUMMARY_INSTRUCTION =
  "Use the selected summary template for the summary structure and section headings.";

// Fork: the E2EE-replicated preferences table went with the cloud layer.
// Every setting now lives in the device-local `app_settings` table.
const SETTING_ROWS_SQL = `
  SELECT id, value_json, 0 AS source_rank FROM app_settings
  ORDER BY id
`;

export function useStoredSettingValuesQuery() {
  return useLiveQuery<AppSettingRow, StoredSettingValues>({
    sql: SETTING_ROWS_SQL,
    mapRows: parseSettingRows,
  });
}

export function useStoredSettingValues(): StoredSettingValues {
  const { data = EMPTY_STORED_SETTINGS } = useStoredSettingValuesQuery();
  return data;
}

export function useSettingsReady(): boolean {
  const { isLoading, error } = useStoredSettingValuesQuery();
  return !isLoading && !error;
}

export function useStoredSettingValue<K extends SettingKey>(
  key: K,
): {
  value: SettingValue<K> | undefined;
  hasValue: boolean;
} {
  const { values, hasValues } = useStoredSettingValues();
  return {
    value: values[key] as SettingValue<K> | undefined,
    hasValue: hasValues.has(key),
  };
}

export async function getStoredSettingValues(): Promise<StoredSettingValues> {
  const rows = await liveQueryClient.execute<AppSettingRow>(SETTING_ROWS_SQL);
  return parseSettingRows(rows);
}

/**
 * A write that only happens if the rows it replaces still hold what was read.
 *
 * Forge 2 (Review 02.09.2026, C3 i): `initializeApplicationSettings` reads
 * once at the start and writes once at the end, with several awaits in
 * between (language detection, template source, retention). A provider the
 * user picks in that window must not be overwritten by the migration of the
 * value that was there before. The check runs inside the "app-settings" write
 * queue, where every settings write is serialised, so nothing can slip in
 * between the re-read and the write.
 *
 * D3 (Fix-Runde 1d): the guard used to cover the migrations only. The STT
 * default model and the language rows were still computed from the FIRST
 * read and written unconditionally -- a provider picked during start-up got
 * the default model of the provider that was there before, and a language
 * chosen in that window was overwritten. Everything that depends on rows a
 * person can change now comes out of `fromLatest`, computed on the re-read
 * inside the queue; `GuardedUpdate` is left for the two decisions that need
 * the first snapshot (an async count, a template source) and only check that
 * their row is still what it was.
 */
type GuardedUpdate = {
  unchanged: Array<[SettingKey, unknown]>;
  write: SettingValues;
};

/**
 * Where a stored STT selection the fork cannot serve moves to.
 *
 * Opus 5 (Review 02.09.2026, C3 iv): the local default is an Apple-Silicon
 * model. On a machine without local transcription the migration would write a
 * pair that `isConfiguredSttModel` accepts and that never runs -- no banner,
 * no transcription. There the selection is cleared instead, and the banner
 * asks for a provider. A selection that does not move is not touched, on any
 * platform.
 */
function migratedSttSelection(
  stored: SettingValues,
  currentPlatform: string,
  currentArch: string,
) {
  // D5 (Fix-Runde 1d): same function as the connection edge
  // (useSTTConnection), and no longer only for a pair that is moving -- a
  // pair already stored under a local provider on a platform without local
  // transcription is cleared as well. A pair that was never set stays unset.
  const servable = resolveServableSttSelection(
    stored.current_stt_provider,
    stored.current_stt_model,
    currentPlatform,
    currentArch,
  );
  if (
    servable.provider === undefined &&
    servable.model === undefined &&
    (stored.current_stt_provider !== undefined ||
      stored.current_stt_model !== undefined)
  ) {
    return { provider: "", model: "" };
  }
  return servable;
}

function selectionWrite(
  current: SettingValues,
  providerKey: "current_stt_provider" | "current_llm_provider",
  modelKey: "current_stt_model" | "current_llm_model",
  target: { provider: string | undefined; model: string | undefined },
): SettingValues {
  const write: SettingValues = {};
  if (target.provider !== current[providerKey]) {
    write[providerKey] = target.provider;
  }
  if (target.model !== current[modelKey]) {
    write[modelKey] = target.model;
  }
  return write;
}

/**
 * Every start-up write that depends on a row a person can change while the
 * start-up tasks run: the STT and LLM selection moves, the STT default model,
 * and the two language rows. Computed on the rows as they are at the moment
 * of writing (D3), never on the first read.
 */
function startupWritesFromLatest(
  latest: StoredSettingValues,
  preferredLanguages: string[] | null,
  currentPlatform: string,
  currentArch: string,
): SettingValues {
  const values: SettingValues = {};

  const sttSelection = migratedSttSelection(
    latest.values,
    currentPlatform,
    currentArch,
  );
  Object.assign(
    values,
    selectionWrite(
      latest.values,
      "current_stt_provider",
      "current_stt_model",
      sttSelection,
    ),
    selectionWrite(
      latest.values,
      "current_llm_provider",
      "current_llm_model",
      normalizeStoredLlmSelection(
        latest.values.current_llm_provider,
        latest.values.current_llm_model,
      ),
    ),
  );

  if (preferredLanguages) {
    if (!latest.hasValues.has("ai_language")) {
      values.ai_language = preferredLanguages[0];
    }

    const storedSpokenLanguages = parseStringArray(
      latest.values.spoken_languages ?? "[]",
    );
    const isSystemDefault =
      latest.values.ai_language === preferredLanguages[0] &&
      storedSpokenLanguages.length === preferredLanguages.length &&
      storedSpokenLanguages.every(
        (language, index) => language === preferredLanguages[index],
      );
    if (!latest.hasValues.has("spoken_languages") || isSystemDefault) {
      values.spoken_languages = JSON.stringify(preferredLanguages.slice(1));
    }
  }

  if (!sttSelection.model) {
    const defaultModel = getDefaultSttModel(sttSelection.provider);
    if (defaultModel) {
      values.current_stt_model = defaultModel;
    }
  }

  return values;
}

export async function initializeApplicationSettings(): Promise<void> {
  const stored = await getStoredSettingValues();
  const languageResult = await detectCommands
    .getPreferredLanguages()
    .catch(() => null);
  const preferredLanguages =
    languageResult?.status === "ok" && languageResult.data.length > 0
      ? languageResult.data
      : null;
  const currentPlatform = platform();
  const currentArch = arch();
  const guarded: GuardedUpdate[] = [];

  const migratedAutoPrompt = await migrateLegacyAutoSummaryPrompt(stored);
  if (migratedAutoPrompt !== null) {
    guarded.push({
      unchanged: [["auto_summary_prompt", stored.values.auto_summary_prompt]],
      write: { auto_summary_prompt: migratedAutoPrompt },
    });
  }

  const retention = await decideAudioRetention(stored);
  if (retention !== null) {
    // The answer was decided on a count taken before the queue; it is only
    // written if nobody answered in the meantime (the consent dialog runs
    // in parallel with start-up).
    guarded.push({
      unchanged: [["audio_retention", undefined]],
      write: { audio_retention: retention },
    });
  }

  const { written, latest } = await writeGuarded(guarded, (current) =>
    startupWritesFromLatest(
      current,
      preferredLanguages,
      currentPlatform,
      currentArch,
    ),
  );
  // Side effects run on what is in the database now -- after our write, or
  // on the re-read when nothing was written -- never on the first snapshot:
  // a switch flipped during start-up would otherwise be flipped back by
  // stale values (D3).
  const current = written ? await getStoredSettingValues() : latest;
  applySettingSideEffects(current.values);
}

async function writeGuarded(
  guarded: GuardedUpdate[],
  fromLatest: (latest: StoredSettingValues) => SettingValues,
): Promise<{ written: boolean; latest: StoredSettingValues }> {
  return enqueueDatabaseWrite("app-settings", async () => {
    const latest = await getStoredSettingValues();
    const values: SettingValues = fromLatest(latest);
    for (const update of guarded) {
      const untouched = update.unchanged.every(
        ([key, value]) => latest.values[key] === value,
      );
      if (untouched) Object.assign(values, update.write);
    }
    if (Object.keys(values).length === 0) return { written: false, latest };
    await persistSettingValues(values);
    return { written: true, latest };
  });
}

/**
 * Writes down how long this machine keeps recordings, the first time it is
 * asked. Returns `null` when there is nothing to write.
 *
 * Mitschnitt-Fork (F17). The retention deadline deletes the recording from the
 * meeting folder as well as from the machine room, so the value it runs on is
 * not a preference any more, it is a licence to delete -- and a licence has to
 * be written down rather than defaulted into existence. Two cases, and the
 * difference between them is the whole point:
 *
 * * The machine already holds meetings. It has been in use, nobody asked its
 *   owner anything, and a fresh six-month default would take a year of
 *   recordings on the next cleanup pass. It keeps everything, in writing.
 * * The machine holds nothing. Nothing can be lost, so it starts on the current
 *   default -- also in writing, because `cleanupExpiredAudio` refuses to run on
 *   an installation whose answer is missing, and that refusal is what makes the
 *   race between this task and the first cleanup pass harmless.
 *
 * A conscious choice is never touched: `hasValues` is true for a direct row and
 * for the legacy `save_recordings` flag alike, so a machine set to thirty days
 * -- or to "don't save" back when that was a switch -- keeps what it said.
 *
 * A database that cannot be counted leaves the answer unwritten. That stops the
 * cleanup rather than guessing, and the next start asks again.
 */
async function decideAudioRetention(
  stored: StoredSettingValues,
): Promise<string | null> {
  if (stored.hasValues.has("audio_retention")) {
    return null;
  }

  // "Don't save recordings" was a switch before it was a deadline, and a
  // machine that still carries only the switch has answered the question --
  // `resolveConfigValue` already reads that pair as "none" (see
  // `~/shared/config`). Writing it down keeps that answer instead of replacing
  // it with a deadline nobody asked for. Reached by a settings bundle exported
  // before the deadline existed; the legacy documents come through `hasValues`
  // above and never get here.
  if (stored.values.save_recordings === false) {
    return "none";
  }

  let holdsMeetings: boolean;
  try {
    const rows = await liveQueryClient.execute<{ any_session: number }>(
      "SELECT EXISTS(SELECT 1 FROM sessions) AS any_session",
    );
    holdsMeetings = rows[0]?.any_session === 1;
  } catch (error) {
    console.error("[settings] could not tell a used machine from a new one", {
      error,
    });
    return null;
  }

  return holdsMeetings
    ? UNASKED_INSTALL_AUDIO_RETENTION
    : DEFAULT_AUDIO_RETENTION;
}

async function migrateLegacyAutoSummaryPrompt(
  stored: StoredSettingValues,
): Promise<string | null> {
  if (
    stored.hasValues.has("auto_summary_prompt") ||
    !stored.hasValues.has("custom_summary_instructions")
  ) {
    return null;
  }

  const legacyInstructions = cleanLegacySummaryInstructions(
    stored.values.custom_summary_instructions ?? "",
  );
  if (!legacyInstructions) {
    return null;
  }

  try {
    const sourceResult =
      await templateCommands.getTemplateSource("enhanceFormat");
    if (sourceResult.status === "error" || !sourceResult.data.trim()) {
      return null;
    }

    return `${sourceResult.data.trimEnd()}\n\n${escapeJinjaOpeners(legacyInstructions)}`;
  } catch {
    return null;
  }
}

function cleanLegacySummaryInstructions(value: string): string {
  return value
    .replace(LEGACY_SUMMARY_TEMPLATE_TOKEN, "")
    .replace(LEGACY_DEFAULT_SUMMARY_INSTRUCTION, "")
    .trim();
}

function escapeJinjaOpeners(value: string): string {
  return value.replace(/\{\{|\{%|\{#/g, (opener) => `{{ "${opener}" }}`);
}

export function setSettingValue<K extends SettingKey>(
  key: K,
  value: SettingValue<K>,
): Promise<void> {
  return setSettingValues({ [key]: value } as SettingValues);
}

export function setSettingValues(values: SettingValues): Promise<void> {
  return enqueueDatabaseWrite("app-settings", () =>
    persistSettingValues(values),
  );
}

export function updateSettingValue<K extends SettingKey>(
  key: K,
  update: (current: SettingValue<K> | undefined) => SettingValue<K>,
): Promise<SettingValue<K>> {
  return enqueueDatabaseWrite("app-settings", async () => {
    const stored = await getStoredSettingValues();
    const definition = SETTING_DEFINITIONS[key];
    const fallback =
      "default" in definition
        ? (definition.default as SettingValue<K>)
        : undefined;
    const current = stored.hasValues.has(key)
      ? (stored.values[key] as unknown as SettingValue<K>)
      : fallback;
    const next = update(current);
    await persistSettingValues({ [key]: next } as SettingValues);
    return next;
  });
}

export function useSetSettingValue<K extends SettingKey>(key: K) {
  return useCallback(
    (value: SettingValue<K>) => {
      void setSettingValue(key, value).catch((error) => {
        console.error(`[settings] failed to update ${key}`, error);
      });
    },
    [key],
  );
}

export function useSetSettingValues() {
  return useCallback((values: SettingValues) => {
    void setSettingValues(values).catch((error) => {
      console.error("[settings] failed to update values", error);
    });
  }, []);
}

export function parseSettingRows(rows: AppSettingRow[]): StoredSettingValues {
  const directRows = new Map(rows.map((row) => [row.id, row.value_json]));
  const legacySettings = parseJsonObject(directRows.get(LEGACY_SETTINGS_ID));
  const legacyMainValues = parseJsonObject(
    directRows.get(LEGACY_MAIN_VALUES_ID),
  );
  const values: SettingValues = {};
  const hasValues = new Set<SettingKey>();

  for (const key of Object.keys(SETTING_DEFINITIONS) as SettingKey[]) {
    const directJson = directRows.get(key);
    const directValue =
      directJson === undefined ? INVALID : parseJsonValue(directJson);
    const normalizedDirect = normalizeSettingValue(key, directValue, true);
    if (normalizedDirect !== INVALID) {
      setParsedSetting(values, key, normalizedDirect);
      hasValues.add(key);
      continue;
    }

    const legacyValue = readLegacySettingValue(
      legacySettings,
      legacyMainValues,
      key,
    );
    const normalizedLegacy = normalizeSettingValue(key, legacyValue, false);
    if (normalizedLegacy !== INVALID) {
      setParsedSetting(values, key, normalizedLegacy);
      hasValues.add(key);
    }
  }

  return { values, hasValues };
}

async function persistSettingValues(values: SettingValues): Promise<void> {
  const now = new Date().toISOString();
  const statements = Object.entries(values).map(([key, value]) => ({
    sql: `
      INSERT INTO app_settings (id, value_json, updated_at)
      VALUES (?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        value_json = excluded.value_json,
        updated_at = excluded.updated_at
    `,
    params: [key, JSON.stringify(value), now],
  }));
  if (statements.length > 0) await executeTransaction(statements);
  applySettingSideEffects(values);
}

function readLegacySettingValue(
  settingsDocument: Record<string, unknown>,
  mainValuesDocument: Record<string, unknown>,
  key: SettingKey,
): unknown {
  const definition = SETTING_DEFINITIONS[key];
  let value = getByPath(settingsDocument, definition.path);

  if (value === undefined && key === "ai_language") {
    value = getByPath(settingsDocument, ["general", "ai_language"]);
  } else if (value === undefined && key === "spoken_languages") {
    value = getByPath(settingsDocument, ["general", "spoken_languages"]);
  } else if (key === "audio_retention") {
    value =
      normalizeAudioRetention(value, undefined) ??
      normalizeAudioRetention(
        getByPath(settingsDocument, ["general", "saveAudioAfterMeeting"]),
        undefined,
      ) ??
      normalizeAudioRetention(
        getByPath(settingsDocument, ["general", "save_recordings"]),
        undefined,
      ) ??
      normalizeAudioRetention(mainValuesDocument.audio_retention, undefined) ??
      normalizeAudioRetention(mainValuesDocument.save_recordings, undefined);
  }

  if (value === undefined) value = mainValuesDocument[key];

  if (key === "current_stt_model") {
    value = normalizeStoredSttModel(
      (getByPath(settingsDocument, ["ai", "current_stt_provider"]) ??
        mainValuesDocument.current_stt_provider) as string | undefined,
      value as string | undefined,
    );
  }

  return value;
}

const INVALID = Symbol("invalid-setting-value");

function normalizeSettingValue(
  key: SettingKey,
  value: unknown,
  direct: boolean,
): boolean | number | string | typeof INVALID {
  if (value === INVALID || value === undefined) return INVALID;

  if (key === "audio_retention") {
    return normalizeAudioRetention(value, undefined) ?? INVALID;
  }

  if (JSON_ARRAY_KEYS.has(key)) {
    if (Array.isArray(value)) return JSON.stringify(value);
    if (typeof value !== "string") return INVALID;
    try {
      return Array.isArray(JSON.parse(value)) ? value : INVALID;
    } catch {
      if (!direct && value.includes(",")) {
        return JSON.stringify(
          value
            .split(",")
            .map((entry) => entry.trim())
            .filter(Boolean),
        );
      }
      return INVALID;
    }
  }

  const expectedType = SETTING_DEFINITIONS[key].type;
  if (expectedType === "boolean" && typeof value === "boolean") return value;
  if (expectedType === "number" && typeof value === "number") return value;
  if (expectedType === "string" && typeof value === "string") return value;
  return INVALID;
}

function setParsedSetting<K extends SettingKey>(
  values: SettingValues,
  key: K,
  value: boolean | number | string,
): void {
  (values as Record<string, unknown>)[key] = value as SettingValue<K>;
}

function getByPath(
  document: Record<string, unknown>,
  path: readonly [string, string],
): unknown {
  const section = document[path[0]];
  return section && typeof section === "object"
    ? (section as Record<string, unknown>)[path[1]]
    : undefined;
}

function parseJsonObject(value: string | undefined): Record<string, unknown> {
  const parsed = parseJsonValue(value);
  return parsed && typeof parsed === "object" && !Array.isArray(parsed)
    ? (parsed as Record<string, unknown>)
    : {};
}

function parseJsonValue(value: string | undefined): unknown {
  if (value === undefined) return INVALID;
  try {
    return JSON.parse(value);
  } catch {
    return INVALID;
  }
}

function applySettingSideEffects(values: SettingValues): void {
  if (values.autostart !== undefined && !isAppStoreBuild()) {
    void (values.autostart ? enable() : disable()).catch(console.error);
  }
  if (values.respect_dnd !== undefined) {
    void detectCommands
      .setRespectDoNotDisturb(values.respect_dnd)
      .catch(console.error);
  }
  if (values.ignored_platforms !== undefined) {
    void detectCommands
      .setIgnoredBundleIds(parseStringArray(values.ignored_platforms))
      .catch(console.error);
  }
  if (values.included_platforms !== undefined) {
    void detectCommands
      .setIncludedBundleIds(parseStringArray(values.included_platforms))
      .catch(console.error);
  }
  if (values.mic_active_threshold !== undefined) {
    void detectCommands
      .setMicActiveThreshold(values.mic_active_threshold)
      .catch(console.error);
  }
  if (values.show_app_in_dock !== undefined) {
    void windowsCommands
      .setShowAppInDock(values.show_app_in_dock)
      .catch(console.error);
  }
  if (values.show_tray_icon !== undefined) {
    void trayCommands
      .setTrayIconVisible(values.show_tray_icon)
      .catch(console.error);
  }
  if (values.automatic_updates !== undefined && !isAppStoreBuild()) {
    void updaterCommands
      .setAutomaticUpdatesEnabled(values.automatic_updates)
      .catch(console.error);
  }
  if (
    values.current_stt_provider !== undefined ||
    values.current_stt_model !== undefined ||
    values.local_stt_model_path !== undefined
  ) {
    void syncLocalSttServer().catch(console.error);
  }
}

async function syncLocalSttServer(): Promise<void> {
  // D3 (Fix-Runde 1d): the repair of a stale local model used to bypass the
  // "app-settings" queue with a raw transaction -- a write racing every
  // other settings write, including the start-up migration above. Read and
  // write now happen inside the queue, and the write is conditional on the
  // row still holding the model that was read.
  const { values } = await enqueueDatabaseWrite("app-settings", async () => {
    const { values } = await getStoredSettingValues();
    const provider = values.current_stt_provider;
    const model = values.current_stt_model;
    if (
      ["soniqo", "apple_speech", "whispercpp", "local_file"].includes(
        provider ?? "",
      ) &&
      model &&
      !isConfiguredSttModel(provider, model)
    ) {
      await executeTransaction([
        {
          sql: `
            UPDATE app_settings
            SET value_json = ?, updated_at = ?
            WHERE id = 'current_stt_model' AND value_json = ?
          `,
          params: [
            JSON.stringify(""),
            new Date().toISOString(),
            JSON.stringify(model),
          ],
        },
      ]);
      return { values: { ...values, current_stt_model: "" } };
    }
    return { values };
  });
  const provider = values.current_stt_provider;
  const model = values.current_stt_model;
  const localModelPath = values.local_stt_model_path?.trim();

  if (provider === "local_file") {
    if (model !== "local-file" || !localModelPath) {
      await localSttCommands.stopServer(null);
    }
    return;
  }

  if (isOnDeviceSttModel(provider, model)) {
    await localSttCommands.startServer(model);
  } else {
    await localSttCommands.stopServer(null);
  }
}

function parseStringArray(value: string): string[] {
  try {
    const parsed = JSON.parse(value);
    return Array.isArray(parsed)
      ? parsed.filter((entry): entry is string => typeof entry === "string")
      : [];
  } catch {
    return [];
  }
}
