import { eq, sql, templates } from "@anlg/db";

import {
  EXPORTED_SETTING_KEYS,
  isExportedSettingKey,
  type ExportedSettingKey,
} from "./keys";
import { isDefaultEndpoint } from "./provider-endpoints";

import { db, liveQueryClient } from "~/db";
import {
  normalizeAudioRetention,
  shortensAudioRetention,
} from "~/services/audio-retention-policy";
import {
  loadSecureAiProviderApiKeys,
  parseAiProviders,
  setAiProvider,
  type AiProviderType,
} from "~/settings/providers";
import { getStoredSettingValues, setSettingValues } from "~/settings/queries";
import { SETTING_DEFINITIONS, type SettingValues } from "~/settings/schema";

/**
 * Mitschnitt-Fork (F14). Collecting a setup into a payload and putting one back.
 *
 * What is NOT here is the point: meetings, recordings and anything else from the
 * database stay behind. The only database table this file reads is `templates`,
 * and it reads it by name, not by scanning.
 */

export type BundleProvider = {
  base_url: string;
  /** Only present when the bundle was exported with credentials. */
  api_key?: string;
};

export type BundleTemplate = {
  id: string;
  title: string;
  description: string;
  category: string | null;
  icon_json: unknown;
  targets_json: unknown;
  sections_json: unknown;
};

export type SettingsBundlePayload = {
  settings: Partial<Record<ExportedSettingKey, boolean | number | string>>;
  providers: Record<AiProviderType, Record<string, BundleProvider>>;
  templates: BundleTemplate[];
};

const PROVIDER_TYPES: AiProviderType[] = ["llm", "stt"];

type AppSettingRow = { id: string; value_json: string };

/** `ai_provider:llm:openai` and friends -- read by name, never by wildcard. */
const PROVIDER_ROW_SQL = `
  SELECT id, value_json FROM app_settings
  WHERE id LIKE 'ai_provider:llm:%'
     OR id LIKE 'ai_provider:stt:%'
     OR id = 'legacy_settings_document'
  ORDER BY id
`;

export async function collectBundlePayload(options: {
  includeSecrets: boolean;
}): Promise<SettingsBundlePayload> {
  const stored = await getStoredSettingValues();
  const settings: SettingsBundlePayload["settings"] = {};
  for (const key of EXPORTED_SETTING_KEYS) {
    if (!stored.hasValues.has(key)) continue;
    const value = stored.values[key];
    if (value === undefined) continue;
    settings[key] = value;
  }

  const rows = await liveQueryClient.execute<AppSettingRow>(PROVIDER_ROW_SQL);
  const providers = {
    llm: {},
    stt: {},
  } as SettingsBundlePayload["providers"];

  for (const type of PROVIDER_TYPES) {
    const parsed = parseAiProviders(rows, type);
    const rowIds = Object.keys(parsed).sort();
    const apiKeys = options.includeSecrets
      ? await loadSecureAiProviderApiKeys(rowIds, type)
      : {};

    for (const rowId of rowIds) {
      const providerId = rowId.slice(`${type}:`.length);
      if (!providerId) continue;
      const baseUrl = parsed[rowId]?.base_url ?? "";
      const apiKey = options.includeSecrets
        ? (apiKeys[rowId] ?? parsed[rowId]?.api_key ?? "")
        : "";

      // A provider with neither an address nor a key carries no setup.
      if (!baseUrl && !apiKey) continue;

      // `api_key` is present only when there is a key to carry. An empty
      // string is NOT "no key" further down the line -- it is an instruction
      // to clear the key, and the importing machine would obey it.
      providers[type][providerId] =
        options.includeSecrets && apiKey
          ? { base_url: baseUrl, api_key: apiKey }
          : { base_url: baseUrl };
    }
  }

  const templateRows = await db
    .select({
      id: templates.id,
      title: templates.title,
      description: templates.description,
      category: templates.category,
      iconJson: templates.iconJson,
      targetsJson: templates.targetsJson,
      sectionsJson: templates.sectionsJson,
    })
    .from(templates)
    .orderBy(templates.id);

  return {
    settings,
    providers,
    templates: templateRows.map((row) => ({
      id: row.id,
      title: row.title,
      description: row.description,
      category: row.category ?? null,
      icon_json: row.iconJson ?? null,
      targets_json: row.targetsJson ?? null,
      sections_json: row.sectionsJson ?? [],
    })),
  };
}

/**
 * Reads a payload that came off disk. A bundle is a file a person can edit, so
 * nothing is trusted: unknown setting keys are dropped rather than written, and
 * every value has to match the type the app declares for that key.
 */
export function parseBundlePayload(raw: unknown): SettingsBundlePayload {
  const document = asObject(raw);
  const settings: SettingsBundlePayload["settings"] = {};

  for (const [key, value] of Object.entries(asObject(document.settings))) {
    if (!isExportedSettingKey(key)) continue;
    const expected = SETTING_DEFINITIONS[key].type;
    if (expected === "boolean" && typeof value !== "boolean") continue;
    if (expected === "number" && typeof value !== "number") continue;
    if (expected === "string" && typeof value !== "string") continue;
    settings[key] = value as boolean | number | string;
  }

  const providers = { llm: {}, stt: {} } as SettingsBundlePayload["providers"];
  const rawProviders = asObject(document.providers);
  for (const type of PROVIDER_TYPES) {
    for (const [providerId, value] of Object.entries(
      asObject(rawProviders[type]),
    )) {
      if (!providerId) continue;
      const entry = asObject(value);
      const baseUrl = typeof entry.base_url === "string" ? entry.base_url : "";
      // An empty (or blank) `api_key` is normalised away rather than carried.
      // Files written before this were exported with `api_key: ""` for every
      // keyless provider, and downstream that string deletes the key already
      // on the importing machine -- while the preview promises the opposite.
      // Normalising here means those files import safely too.
      const rawApiKey = typeof entry.api_key === "string" ? entry.api_key : "";
      const apiKey = rawApiKey.trim() === "" ? undefined : rawApiKey;
      if (!baseUrl && !apiKey) continue;
      providers[type][providerId] =
        apiKey === undefined
          ? { base_url: baseUrl }
          : { base_url: baseUrl, api_key: apiKey };
    }
  }

  const rawTemplates = Array.isArray(document.templates)
    ? document.templates
    : [];
  const parsedTemplates: BundleTemplate[] = [];
  for (const value of rawTemplates) {
    const row = asObject(value);
    if (typeof row.id !== "string" || !row.id) continue;
    if (!Array.isArray(row.sections_json)) continue;
    parsedTemplates.push({
      id: row.id,
      title: typeof row.title === "string" ? row.title : "",
      description: typeof row.description === "string" ? row.description : "",
      category: typeof row.category === "string" ? row.category : null,
      icon_json: row.icon_json ?? null,
      targets_json: row.targets_json ?? null,
      sections_json: row.sections_json,
    });
  }

  return { settings, providers, templates: parsedTemplates };
}

export type SettingChange = {
  key: ExportedSettingKey;
  before: boolean | number | string | undefined;
  after: boolean | number | string;
};

export type ProviderChange = {
  type: AiProviderType;
  providerId: string;
  carriesKey: boolean;
  action: "add" | "update";
  /**
   * The address this provider would be pointed at.
   *
   * On the plan, and shown, because it is the most dangerous field in a bundle
   * and it used to be invisible: the preview said "with/without key" and
   * nothing else, so a readable, password-free bundle -- the kind meant to be
   * handed around -- could set `base_url` to a host of the writer's choosing
   * and the person applying it saw nothing unusual.
   */
  baseUrl: string;
  /**
   * False when the address is not the one the app itself uses for this
   * provider. Requires the person's explicit agreement before anything is
   * written; see `applyImport`.
   */
  isDefaultEndpoint: boolean;
  /**
   * The bundle brings no key for this provider, this machine already has one,
   * and the address is not the provider's own -- so applying would send the
   * key that is already here to somebody else's host, along with everything
   * that goes through it.
   */
  sendsExistingKeyElsewhere: boolean;
};

export type TemplateChange = {
  id: string;
  title: string;
  action: "add" | "replace";
};

export type ImportPlan = {
  settings: SettingChange[];
  providers: ProviderChange[];
  templates: TemplateChange[];
  unchangedSettings: number;
  /**
   * Set when the bundle selects a summary template that neither travels with
   * it nor exists here. The import still applies -- the setting is honest
   * about what the source machine had -- but the summary would fall back
   * silently, and a preview that does not say so is a preview that lies.
   */
  missingSelectedTemplateId: string | null;
};

/** What the import would change, worked out before anything is written. */
export async function planImport(
  payload: SettingsBundlePayload,
): Promise<ImportPlan> {
  const stored = await getStoredSettingValues();
  const settings: SettingChange[] = [];
  let unchangedSettings = 0;

  for (const key of EXPORTED_SETTING_KEYS) {
    const after = payload.settings[key];
    if (after === undefined) continue;
    const before = stored.hasValues.has(key) ? stored.values[key] : undefined;
    if (before === after) {
      unchangedSettings += 1;
      continue;
    }
    settings.push({ key, before, after });
  }

  const rows = await liveQueryClient.execute<AppSettingRow>(PROVIDER_ROW_SQL);
  const providers: ProviderChange[] = [];
  for (const type of PROVIDER_TYPES) {
    const existing = parseAiProviders(rows, type);
    const heldKeys = await loadSecureAiProviderApiKeys(
      Object.keys(existing),
      type,
    );
    for (const [providerId, entry] of Object.entries(payload.providers[type])) {
      const rowId = `${type}:${providerId}`;
      const carriesKey = Boolean(entry.api_key);
      const known = isDefaultEndpoint(type, providerId, entry.base_url);
      providers.push({
        type,
        providerId,
        carriesKey,
        action: existing[rowId] ? "update" : "add",
        baseUrl: entry.base_url,
        isDefaultEndpoint: known,
        sendsExistingKeyElsewhere:
          !known && !carriesKey && Boolean(heldKeys[rowId]),
      });
    }
  }

  const existingTemplateIds = new Set(
    (await db.select({ id: templates.id }).from(templates)).map(
      (row) => row.id,
    ),
  );
  const templateChanges: TemplateChange[] = payload.templates.map(
    (template) => ({
      id: template.id,
      title: template.title,
      action: existingTemplateIds.has(template.id) ? "replace" : "add",
    }),
  );

  const selectedTemplateId = payload.settings.selected_template_id;
  const missingSelectedTemplateId =
    typeof selectedTemplateId === "string" &&
    selectedTemplateId !== "" &&
    !payload.templates.some((template) => template.id === selectedTemplateId) &&
    !existingTemplateIds.has(selectedTemplateId)
      ? selectedTemplateId
      : null;

  return {
    settings,
    providers,
    templates: templateChanges,
    unchangedSettings,
    missingSelectedTemplateId,
  };
}

/** The three stages an import writes, in the order it writes them. */
export type ImportStage = "templates" | "providers" | "settings";

/**
 * An import that stopped partway, carrying WHICH stages were already written.
 *
 * There is no honest way to make this atomic: templates live in SQLite, keys
 * live in the OS keychain, settings live in a third place, and no transaction
 * spans them. So instead of pretending, the failure names what happened -- and
 * every stage is written so that running the import again lands in the same
 * place (templates upsert, providers and settings overwrite by key).
 */
export class PartialImportError extends Error {
  constructor(
    readonly stage: ImportStage,
    readonly applied: ImportStage[],
    readonly pending: ImportStage[],
    readonly cause: unknown,
    /**
     * Names of the items INSIDE `stage` that were written before it failed.
     * `templates` and `providers` write one item at a time, so a failure on
     * the second item still leaves the first one on disk -- that item's name
     * belongs here, not just silently inside a stage the caller is told is
     * "pending". Empty when the stage never got to write anything (it failed
     * on its first item, or it is `settings`, which writes in one call).
     */
    readonly appliedItems: string[] = [],
  ) {
    super(`settings import stopped in stage "${stage}"`);
    this.name = "PartialImportError";
  }
}

/** The order the stages run in. Templates first, because a template is the
 * only stage another stage can point AT: `selected_template_id` in the
 * settings would otherwise name a template that is not there yet. Settings
 * last for the same reason with respect to providers. */
const IMPORT_STAGES: ImportStage[] = ["templates", "providers", "settings"];

/**
 * Thrown when a bundle would point a provider somewhere other than the
 * provider's own address and nobody has agreed to that.
 *
 * The address is the field in a bundle that can do the most damage, and it can
 * do it from the shape of bundle that looks harmless: readable JSON, no
 * password, "no credentials". It does not need to carry a key -- the importing
 * machine keeps its own, which is exactly right, and then sends it, and the
 * contents of every meeting it summarises or transcribes, wherever the file
 * says. So it is not enough to show the address; applying has to be an act a
 * person performed on purpose.
 *
 * Refusing outright would break the honest version of the same setup (a company
 * proxy, LM Studio, Ollama on another host), which is a real and ordinary thing
 * to put in a bundle. So the gate is a confirmation rather than a ban -- and
 * because the confirmation is required, the DEFAULT outcome of an unattended or
 * careless import is that nothing is written and no key goes anywhere.
 */
export class UnconfirmedEndpointError extends Error {
  constructor(readonly providers: ProviderChange[]) {
    super("the import points providers at addresses that were not confirmed");
    this.name = "UnconfirmedEndpointError";
  }
}

export type ApplyImportOptions = {
  /**
   * Set once the person has seen the addresses in the preview and agreed to
   * them. Anything else -- including simply not passing the option -- means the
   * import stops before its first write.
   */
  confirmedEndpoints?: boolean;
};

/**
 * Writes the payload.
 *
 * Everything that can be checked is checked before the first write, so the
 * common failure (a bundle that does not fit this machine) costs nothing. What
 * cannot be prevented -- a keychain that refuses halfway, a database that
 * locks -- surfaces as `PartialImportError` naming the stages already written.
 */
export async function applyImport(
  payload: SettingsBundlePayload,
  options: ApplyImportOptions = {},
): Promise<void> {
  // Before the first write, and from the payload itself rather than from a plan
  // the caller hands in: a caller that computed the plan and then applied a
  // different payload would walk straight past the gate.
  if (!options.confirmedEndpoints) {
    const unconfirmed: ProviderChange[] = [];
    for (const type of PROVIDER_TYPES) {
      for (const [providerId, entry] of Object.entries(
        payload.providers[type],
      )) {
        if (isDefaultEndpoint(type, providerId, entry.base_url)) continue;
        unconfirmed.push({
          type,
          providerId,
          carriesKey: Boolean(entry.api_key),
          action: "update",
          baseUrl: entry.base_url,
          isDefaultEndpoint: false,
          sendsExistingKeyElsewhere: !entry.api_key,
        });
      }
    }
    if (unconfirmed.length > 0) {
      throw new UnconfirmedEndpointError(unconfirmed);
    }
  }

  const settingKeys = Object.keys(payload.settings);
  // Each runner is handed the array it should record its OWN progress into,
  // item by item, as it writes -- not just a pass/fail for the whole stage.
  // `templates` and `providers` write one item at a time and can fail
  // partway through; `settings` writes in a single call and has nothing to
  // report between "none" and "all".
  const stages: Record<ImportStage, (appliedItems: string[]) => Promise<void>> =
    {
      templates: async (appliedItems) => {
        for (const template of payload.templates) {
          await upsertTemplate(template);
          appliedItems.push(template.title || template.id);
        }
      },
      providers: async (appliedItems) => {
        for (const type of PROVIDER_TYPES) {
          for (const [providerId, entry] of Object.entries(
            payload.providers[type],
          )) {
            await setAiProvider(type, providerId, {
              base_url: entry.base_url,
              // Leaving `api_key` out keeps whatever key is already on this
              // machine, which is what a bundle without credentials should do.
              // `parseBundlePayload` guarantees it is either absent or non-empty
              // -- an empty string here would DELETE the key instead.
              ...(entry.api_key === undefined
                ? {}
                : { api_key: entry.api_key }),
            });
            appliedItems.push(`${type}:${providerId}`);
          }
        }
      },
      settings: async () => {
        if (settingKeys.length > 0) {
          await setSettingValues({
            ...(payload.settings as SettingValues),
            ...(await disarmRetentionIfImportShortens(payload)),
          });
        }
      },
    };

  const applied: ImportStage[] = [];
  for (const [index, stage] of IMPORT_STAGES.entries()) {
    const appliedItems: string[] = [];
    try {
      await stages[stage](appliedItems);
    } catch (cause) {
      throw new PartialImportError(
        stage,
        applied,
        IMPORT_STAGES.slice(index),
        cause,
        appliedItems,
      );
    }
    applied.push(stage);
  }
}

/**
 * Takes the deadline off its safety catch again when an import shortens it.
 *
 * Mitschnitt-Fork (F18). This is the second door into the retention deadline,
 * and it is a quieter one than the picker: an import writes `audio_retention`
 * with no dialog at all, and the preview lists it as one changed setting among
 * twenty. On a machine that already agreed to the deadline once, a shorter
 * imported value would therefore start deleting on the next pass -- for a span
 * nobody on THIS machine ever looked at.
 *
 * An imported value is a setting, not an answer. So the answer is withdrawn and
 * the cleanup pass asks once more, with the count, exactly as it would after a
 * restore. No new dialog in the import: the right effect, in the place that
 * already knows how to ask.
 *
 * `save_recordings` alone cannot get here. It only reads as "don't save" while
 * `audio_retention` is unset (see `resolveConfigValue` in `~/shared/config`),
 * and a machine that has confirmed its deadline has that value written down.
 */
async function disarmRetentionIfImportShortens(
  payload: SettingsBundlePayload,
): Promise<SettingValues> {
  const imported = normalizeAudioRetention(
    payload.settings.audio_retention,
    undefined,
  );
  if (imported === undefined) return {};

  const stored = await getStoredSettingValues();
  // Nothing to withdraw on a machine that never gave the answer -- and it is
  // already in the safe state, so writing `false` would only add noise.
  if (stored.values.audio_retention_confirmed !== true) return {};

  // "forever", not the six-month default, and not by accident: every other
  // place that has to read an unreadable or missing deadline chooses "forever"
  // on purpose (`retentionForCleanup`, and the picker's `|| "forever"`), and a
  // different answer here runs the unsafe way. A machine whose stored row is
  // corrupt deletes nothing today; comparing an import against "sixMonths"
  // would call an incoming one-year deadline LONGER, leave the answer armed,
  // and turn "delete nothing" into "delete everything past a year" in silence.
  // Found by the second-look review.
  const current = normalizeAudioRetention(
    stored.values.audio_retention,
    "forever",
  );
  return shortensAudioRetention(current, imported)
    ? { audio_retention_confirmed: false }
    : {};
}

async function upsertTemplate(template: BundleTemplate): Promise<void> {
  const existing = await db
    .select({ id: templates.id })
    .from(templates)
    .where(eq(templates.id, template.id))
    .limit(1);

  const values = {
    title: template.title,
    description: template.description,
    category: template.category,
    iconJson: template.icon_json,
    targetsJson: template.targets_json,
    sectionsJson: template.sections_json,
  };

  if (existing.length > 0) {
    await db
      .update(templates)
      .set({
        ...values,
        updatedAt: sql`strftime('%Y-%m-%dT%H:%M:%SZ', 'now')`,
      })
      .where(eq(templates.id, template.id));
    return;
  }

  await db.insert(templates).values({
    id: template.id,
    ...values,
    pinned: false,
    createdAt: sql`strftime('%Y-%m-%dT%H:%M:%SZ', 'now')`,
    updatedAt: sql`strftime('%Y-%m-%dT%H:%M:%SZ', 'now')`,
  });
}

function asObject(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}
