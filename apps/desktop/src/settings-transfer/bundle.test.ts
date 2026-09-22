import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  execute: vi.fn(),
  executeTransaction: vi.fn(
    (_statements: Array<{ sql: string; params: unknown[] }>) =>
      Promise.resolve([1]),
  ),
  getSecret: vi.fn(async () => ({ status: "ok", data: null as string | null })),
  setSecret: vi.fn(async () => ({ status: "ok", data: null })),
  deleteSecret: vi.fn(async () => ({ status: "ok", data: null })),
  getStoredSettingValues: vi.fn(),
  setSettingValues: vi.fn(async () => undefined),
  templateRows: [] as Array<Record<string, unknown>>,
  templateInserts: [] as Array<Record<string, unknown>>,
  templateUpdates: [] as Array<{
    id: unknown;
    values: Record<string, unknown>;
  }>,
  // Set to the 0-based index of the insert call that should reject, to
  // simulate the SECOND (or later) item of the templates stage failing
  // while an earlier item already made it to disk. `null` means every
  // insert succeeds.
  templateInsertFailOn: null as number | null,
}));

vi.mock("@anlg/plugin-store2", () => ({
  commands: {
    getSecret: mocks.getSecret,
    setSecret: mocks.setSecret,
    deleteSecret: mocks.deleteSecret,
  },
}));

vi.mock("~/db/write-queue", () => ({
  enqueueDatabaseWrite: (_key: string, operation: () => Promise<unknown>) =>
    operation(),
}));

vi.mock("~/settings/queries", () => ({
  getStoredSettingValues: mocks.getStoredSettingValues,
  setSettingValues: mocks.setSettingValues,
}));

// A hand-rolled stand-in for the drizzle handle, small enough to read and
// strict enough to notice a query against any table but `templates`.
vi.mock("@anlg/db", () => ({
  templates: { __table: "templates", id: { name: "id" } },
  eq: (_column: unknown, value: unknown) => ({ eq: value }),
  sql: (strings: TemplateStringsArray) => ({ sql: strings.join("") }),
}));

vi.mock("~/db", () => {
  const selectResult = (table: { __table?: string }) => {
    if (table?.__table !== "templates") {
      throw new Error(`the bundle must not read the table ${table?.__table}`);
    }
    const rows = mocks.templateRows;
    const chain = {
      orderBy: () => Promise.resolve(rows),
      where: (condition: { eq: unknown }) => ({
        limit: () =>
          Promise.resolve(rows.filter((row) => row.id === condition.eq)),
      }),
      then: (resolve: (value: unknown) => unknown) => resolve(rows),
    };
    return chain;
  };

  return {
    liveQueryClient: { execute: mocks.execute },
    executeTransaction: mocks.executeTransaction,
    db: {
      select: () => ({ from: selectResult }),
      insert: (table: { __table?: string }) => ({
        values: (values: Record<string, unknown>) => {
          if (table?.__table !== "templates") {
            throw new Error("the import must only write templates");
          }
          if (mocks.templateInserts.length === mocks.templateInsertFailOn) {
            return Promise.reject(new Error("disk full"));
          }
          mocks.templateInserts.push(values);
          return Promise.resolve();
        },
      }),
      update: () => ({
        set: (values: Record<string, unknown>) => ({
          where: (condition: { eq: unknown }) => {
            mocks.templateUpdates.push({ id: condition.eq, values });
            return Promise.resolve();
          },
        }),
      }),
    },
  };
});

import {
  applyImport,
  collectBundlePayload,
  parseBundlePayload,
  PartialImportError,
  planImport,
  UnconfirmedEndpointError,
} from "./bundle";
import {
  EXPORTED_SETTING_KEYS,
  LEGACY_WITHHELD_SETTING_KEYS,
  WITHHELD_SETTING_KEYS,
} from "./keys";

import { SETTING_DEFINITIONS, type SettingKey } from "~/settings/schema";

const PROVIDER_ROWS = [
  {
    id: "ai_provider:stt:openai",
    value_json: JSON.stringify({
      base_url: "https://api.openai.com/v1",
      api_key: "",
    }),
  },
  {
    id: "ai_provider:llm:openrouter",
    value_json: JSON.stringify({
      base_url: "https://openrouter.ai/api/v1",
      api_key: "",
    }),
  },
];

function storedSettings(values: Record<string, unknown>) {
  return {
    values,
    hasValues: new Set(Object.keys(values) as SettingKey[]),
  };
}

const SOURCE_SETTINGS = {
  current_stt_provider: "whispercpp",
  current_stt_model: "QuantizedLargeV3Turbo",
  ai_language: "de",
  personalization_dictionary_terms: JSON.stringify(["Sedacz", "Nordwerk"]),
  theme: "light",
  // Present on the source machine but deliberately not exportable.
  telemetry_consent: false,
  microphone_device: "MacBook Pro Microphone",
  automation_slack_recap_channel: "C0815",
  local_stt_model_path: "/Users/mads/models/ggml-large-v3.bin",
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.templateRows = [];
  mocks.templateInserts = [];
  mocks.templateUpdates = [];
  mocks.templateInsertFailOn = null;
  mocks.execute.mockResolvedValue(PROVIDER_ROWS);
  mocks.getStoredSettingValues.mockResolvedValue(
    storedSettings(SOURCE_SETTINGS),
  );
  mocks.getSecret.mockResolvedValue({ status: "ok", data: "sk-geheim-4711" });
});

describe("the allowlist", () => {
  it("has a decision for every setting the app knows", () => {
    // Without this, a setting upstream adds would sit in neither list and
    // nobody would notice until it either leaked or went missing.
    const undecided = (Object.keys(SETTING_DEFINITIONS) as SettingKey[]).filter(
      (key) =>
        !(EXPORTED_SETTING_KEYS as readonly string[]).includes(key) &&
        !(key in WITHHELD_SETTING_KEYS),
    );

    expect(undecided).toEqual([]);
  });

  // F16 falsifier 5. The mirror root is a folder on one machine -- carrying it
  // into a bundle would point another machine's meetings at a path it has no
  // business writing to. The completeness test above only forces a decision;
  // this one names which way the decision went.
  it("keeps the meeting folder out of a bundle", () => {
    expect(EXPORTED_SETTING_KEYS).not.toContain("mirror_root");
    expect(WITHHELD_SETTING_KEYS.mirror_root).toBeTruthy();
  });

  it("never both exports and withholds the same setting", () => {
    const contradictory = EXPORTED_SETTING_KEYS.filter(
      (key) => key in WITHHELD_SETTING_KEYS,
    );

    expect(contradictory).toEqual([]);
  });
});

describe("collecting a bundle", () => {
  it("takes the setup and leaves the rest behind", async () => {
    const payload = await collectBundlePayload({ includeSecrets: false });

    expect(payload.settings.current_stt_model).toBe("QuantizedLargeV3Turbo");
    expect(payload.settings.personalization_dictionary_terms).toContain(
      "Sedacz",
    );
    // Falsifier 3, at the level where it is decidable: keys that exist on the
    // source machine and must not travel.
    expect(payload.settings).not.toHaveProperty("telemetry_consent");
    expect(payload.settings).not.toHaveProperty("microphone_device");
    expect(payload.settings).not.toHaveProperty(
      "automation_slack_recap_channel",
    );
    // Forge-Audit: a filesystem path only means anything on the machine that
    // wrote it. On the target machine it points into a directory that likely
    // does not exist, or worse, exists and holds something else.
    expect(payload.settings).not.toHaveProperty("local_stt_model_path");
  });

  it("reads only app_settings and templates", async () => {
    await collectBundlePayload({ includeSecrets: false });

    // The drizzle stand-in throws on any other table; this covers the raw SQL.
    for (const [sqlText] of mocks.execute.mock.calls) {
      expect(sqlText).toContain("FROM app_settings");
      for (const table of [
        "sessions",
        "transcripts",
        "session_participants",
        "session_documents",
        "humans",
      ]) {
        expect(String(sqlText)).not.toContain(table);
      }
    }
  });

  it("leaves the keys out unless credentials were asked for", async () => {
    const withoutSecrets = await collectBundlePayload({
      includeSecrets: false,
    });
    expect(withoutSecrets.providers.stt.openai).toEqual({
      base_url: "https://api.openai.com/v1",
    });
    expect(JSON.stringify(withoutSecrets)).not.toContain("sk-geheim-4711");
    expect(mocks.getSecret).not.toHaveBeenCalled();

    const withSecrets = await collectBundlePayload({ includeSecrets: true });
    expect(withSecrets.providers.stt.openai?.api_key).toBe("sk-geheim-4711");
    expect(withSecrets.providers.llm.openrouter?.api_key).toBe(
      "sk-geheim-4711",
    );
  });

  it("carries the summary templates but not what was written with them", async () => {
    mocks.templateRows = [
      {
        id: "tpl-1",
        title: "Kundentermin",
        description: "",
        category: null,
        iconJson: null,
        targetsJson: null,
        sectionsJson: [{ title: "Entscheidungen", description: "" }],
      },
    ];

    const payload = await collectBundlePayload({ includeSecrets: false });

    expect(payload.templates).toHaveLength(1);
    expect(payload.templates[0]?.title).toBe("Kundentermin");
    expect(Object.keys(payload.templates[0] ?? {})).toEqual([
      "id",
      "title",
      "description",
      "category",
      "icon_json",
      "targets_json",
      "sections_json",
    ]);
  });
});

describe("reading a bundle that came off disk", () => {
  it("drops settings the allowlist does not cover", () => {
    const payload = parseBundlePayload({
      settings: {
        theme: "dark",
        // Someone edited the file by hand to flip a consent flag.
        telemetry_consent: true,
        crash_reporting_consent: true,
        lock_app: false,
      },
    });

    expect(payload.settings).toEqual({ theme: "dark" });
  });

  it("drops values of the wrong type instead of writing them", () => {
    const payload = parseBundlePayload({
      settings: {
        theme: 7,
        floating_bar_opacity: "sehr durchsichtig",
        save_recordings: "ja",
        mic_active_threshold: 22,
      },
    });

    expect(payload.settings).toEqual({ mic_active_threshold: 22 });
  });

  it("drops templates without an id or without sections", () => {
    const payload = parseBundlePayload({
      templates: [
        { id: "", sections_json: [] },
        { id: "tpl-2" },
        { id: "tpl-3", sections_json: [], title: "Leer" },
      ],
    });

    expect(payload.templates.map((template) => template.id)).toEqual(["tpl-3"]);
  });

  it("survives a file with nothing in it", () => {
    expect(parseBundlePayload(null)).toEqual({
      settings: {},
      providers: { llm: {}, stt: {} },
      templates: [],
    });
  });
});

describe("the preview before anything is written", () => {
  it("names what changes and counts what does not", async () => {
    mocks.getStoredSettingValues.mockResolvedValue(
      storedSettings({ theme: "light", ai_language: "de" }),
    );

    const plan = await planImport(
      parseBundlePayload({
        settings: { theme: "dark", ai_language: "de", app_icon: "classic" },
        providers: { stt: { openai: { base_url: "", api_key: "sk-neu" } } },
        templates: [{ id: "tpl-1", sections_json: [], title: "Kundentermin" }],
      }),
    );

    // The order follows the allowlist, not the file, so the preview reads the
    // same no matter how the bundle happened to be serialized.
    expect(plan.settings).toEqual([
      { key: "theme", before: "light", after: "dark" },
      { key: "app_icon", before: undefined, after: "classic" },
    ]);
    expect(plan.unchangedSettings).toBe(1);
    expect(plan.providers).toEqual([
      {
        type: "stt",
        providerId: "openai",
        carriesKey: true,
        action: "update",
        // Empty means "the app decides", which is the provider's own address.
        baseUrl: "",
        isDefaultEndpoint: true,
        sendsExistingKeyElsewhere: false,
      },
    ]);
    expect(plan.templates).toEqual([
      { id: "tpl-1", title: "Kundentermin", action: "add" },
    ]);
  });

  it("says whether a template would replace an existing one", async () => {
    mocks.templateRows = [{ id: "tpl-1" }];

    const plan = await planImport(
      parseBundlePayload({
        templates: [{ id: "tpl-1", sections_json: [], title: "Kundentermin" }],
      }),
    );

    expect(plan.templates[0]?.action).toBe("replace");
  });
});

describe("applying a bundle on a fresh machine", () => {
  it("gives the target the key, the template and the word list", async () => {
    // Falsifier 1: a machine that has nothing yet.
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(storedSettings({}));
    mocks.getSecret.mockResolvedValue({ status: "ok", data: null });

    await applyImport(
      parseBundlePayload({
        settings: {
          current_stt_provider: "openai",
          personalization_dictionary_terms: JSON.stringify(["Sedacz"]),
        },
        providers: {
          stt: {
            openai: {
              base_url: "https://api.openai.com/v1",
              api_key: "sk-geheim-4711",
            },
          },
        },
        templates: [{ id: "tpl-1", sections_json: [], title: "Kundentermin" }],
      }),
    );

    expect(mocks.setSecret).toHaveBeenCalledWith(
      "ai-provider-api-keys",
      "stt:openai",
      "sk-geheim-4711",
    );
    expect(mocks.templateInserts).toHaveLength(1);
    expect(mocks.templateInserts[0]).toMatchObject({
      id: "tpl-1",
      title: "Kundentermin",
    });
    expect(mocks.setSettingValues).toHaveBeenCalledWith({
      current_stt_provider: "openai",
      personalization_dictionary_terms: JSON.stringify(["Sedacz"]),
    });
  });

  it("keeps the key already on this machine when the file has none", async () => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(storedSettings({}));

    await applyImport(
      parseBundlePayload({
        providers: {
          stt: { openai: { base_url: "https://api.openai.com/v1" } },
        },
      }),
    );

    // setAiProvider without an api_key reuses the stored one; it must never
    // write an empty string, which would delete the key.
    expect(mocks.deleteSecret).not.toHaveBeenCalled();
    expect(mocks.setSecret).toHaveBeenCalledWith(
      "ai-provider-api-keys",
      "stt:openai",
      "sk-geheim-4711",
    );
  });

  it("overwrites a template of the same id instead of duplicating it", async () => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(storedSettings({}));
    mocks.templateRows = [{ id: "tpl-1" }];

    await applyImport(
      parseBundlePayload({
        templates: [{ id: "tpl-1", sections_json: [], title: "Neu" }],
      }),
    );

    expect(mocks.templateInserts).toHaveLength(0);
    expect(mocks.templateUpdates).toHaveLength(1);
    expect(mocks.templateUpdates[0]?.values).toMatchObject({ title: "Neu" });
  });

  // B1. The sibling test above covers the bundle with NO `api_key` field. This
  // is the shape the exporter actually wrote before the fix: the field present
  // and empty. It reached `setAiProvider` as `api_key: ""`, where
  // `changes.api_key ?? previousApiKey` does not fall back (an empty string is
  // not nullish) and `setProviderApiKey` turns "" into `deleteSecret`. So the
  // import DELETED the key on the target machine -- while the preview said
  // "without access key".
  it("keeps the key already on this machine when the file carries an empty one", async () => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(storedSettings({}));

    const payload = parseBundlePayload({
      providers: {
        stt: { openai: { base_url: "https://api.openai.com/v1", api_key: "" } },
      },
    });

    expect(payload.providers.stt.openai).not.toHaveProperty("api_key");

    const plan = await planImport(payload);
    expect(plan.providers[0]?.carriesKey).toBe(false);

    await applyImport(payload);

    expect(mocks.deleteSecret).not.toHaveBeenCalled();
    expect(mocks.setSecret).toHaveBeenCalledWith(
      "ai-provider-api-keys",
      "stt:openai",
      "sk-geheim-4711",
    );
  });

  // Whitespace is the same mistake wearing a hat: truthy, so it survives every
  // emptiness check, and would be stored as the new key.
  it("treats a blank key in the file as no key", async () => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(storedSettings({}));

    await applyImport(
      parseBundlePayload({
        providers: {
          stt: {
            openai: { base_url: "https://api.openai.com/v1", api_key: "   " },
          },
        },
      }),
    );

    expect(mocks.deleteSecret).not.toHaveBeenCalled();
    expect(mocks.setSecret).toHaveBeenCalledWith(
      "ai-provider-api-keys",
      "stt:openai",
      "sk-geheim-4711",
    );
  });

  // The export side of B1: a keyless provider must not get the field at all,
  // so no future reader of the file has to know the empty-string rule.
  it("never writes an empty key into the file", async () => {
    mocks.execute.mockResolvedValue(PROVIDER_ROWS);
    mocks.getSecret.mockResolvedValue({ status: "ok", data: null });

    const payload = await collectBundlePayload({ includeSecrets: true });

    for (const type of ["llm", "stt"] as const) {
      for (const entry of Object.values(payload.providers[type])) {
        expect(entry).not.toHaveProperty("api_key");
      }
    }
    expect(JSON.stringify(payload)).not.toContain('"api_key":""');
  });
});

// E2. Three stores, no shared transaction. The honest answer is not to claim
// atomicity but to name what happened and make a second run safe.
describe("an import that stops partway", () => {
  it("names the stages already applied and the ones still pending", async () => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(storedSettings({}));
    mocks.setSecret.mockRejectedValueOnce(new Error("keychain locked"));

    const payload = parseBundlePayload({
      templates: [{ id: "tpl-1", sections_json: [], title: "Kundentermin" }],
      providers: {
        stt: {
          openai: { base_url: "https://api.openai.com/v1", api_key: "sk-a" },
        },
      },
      settings: { ai_language: "de" },
    });

    const failure: unknown = await applyImport(payload).then(
      () => new Error("the import was expected to fail"),
      (cause: unknown) => cause,
    );

    expect(failure).toBeInstanceOf(PartialImportError);
    if (!(failure instanceof PartialImportError)) return;
    expect(failure.stage).toBe("providers");
    expect(failure.applied).toEqual(["templates"]);
    expect(failure.pending).toEqual(["providers", "settings"]);
    // The stage after the failure must not have run.
    expect(mocks.setSettingValues).not.toHaveBeenCalled();
  });

  // B2. `applied` only ever named whole STAGES. A stage that writes one item
  // at a time (templates, providers) can itself fail partway through -- the
  // first item already sits in the database when the second one throws, but
  // the old shape reported the templates stage as entirely "pending", which
  // read as "nothing was applied yet" even though a template had landed.
  // The bug was invisible to the sibling test above because it only fails
  // the FIRST provider of a single-item stage; this one fails the SECOND
  // item of a two-item stage.
  it("names the item already written when a stage fails on its second item", async () => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(storedSettings({}));
    // The first template writes fine; the second is where the loop breaks.
    mocks.templateInsertFailOn = 1;

    const payload = parseBundlePayload({
      templates: [
        { id: "tpl-1", sections_json: [], title: "Kundentermin" },
        { id: "tpl-2", sections_json: [], title: "Standup" },
      ],
    });

    const failure: unknown = await applyImport(payload).then(
      () => new Error("the import was expected to fail"),
      (cause: unknown) => cause,
    );

    expect(failure).toBeInstanceOf(PartialImportError);
    if (!(failure instanceof PartialImportError)) return;
    expect(failure.stage).toBe("templates");
    // The stage as a whole did not complete...
    expect(failure.applied).toEqual([]);
    expect(failure.pending).toEqual(["templates", "providers", "settings"]);
    // ...but the first template is on disk, and the failure says so by name.
    expect(failure.appliedItems).toEqual(["Kundentermin"]);
    expect(mocks.templateInserts).toHaveLength(1);
  });

  it("reaches the intended state on a second run, without duplicates", async () => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(storedSettings({}));
    mocks.setSecret.mockRejectedValueOnce(new Error("keychain locked"));

    const payload = parseBundlePayload({
      templates: [{ id: "tpl-1", sections_json: [], title: "Kundentermin" }],
      providers: {
        stt: {
          openai: { base_url: "https://api.openai.com/v1", api_key: "sk-a" },
        },
      },
      settings: { ai_language: "de" },
    });

    await expect(applyImport(payload)).rejects.toBeInstanceOf(
      PartialImportError,
    );

    // The template the first run wrote now exists, so the second run has to
    // update it rather than insert a twin.
    mocks.templateRows = [{ id: "tpl-1" }];
    await expect(applyImport(payload)).resolves.toBeUndefined();

    expect(mocks.templateInserts).toHaveLength(1);
    expect(mocks.templateUpdates).toHaveLength(1);
    expect(mocks.setSettingValues).toHaveBeenCalledWith({ ai_language: "de" });
  });
});

// K5. `selected_template_id` points at a template that neither travels with
// the bundle nor exists here: the import still applies, but the preview has to
// say so instead of letting summaries fall back silently.
describe("a selected template that is nowhere", () => {
  beforeEach(() => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(storedSettings({}));
  });

  it("is named in the plan", async () => {
    const plan = await planImport(
      parseBundlePayload({ settings: { selected_template_id: "tpl-weg" } }),
    );

    expect(plan.missingSelectedTemplateId).toBe("tpl-weg");
  });

  it("stays quiet when the template travels with the bundle", async () => {
    const plan = await planImport(
      parseBundlePayload({
        settings: { selected_template_id: "tpl-1" },
        templates: [{ id: "tpl-1", sections_json: [], title: "Kundentermin" }],
      }),
    );

    expect(plan.missingSelectedTemplateId).toBeNull();
  });

  it("stays quiet when the template is already on this machine", async () => {
    mocks.templateRows = [{ id: "tpl-1" }];

    const plan = await planImport(
      parseBundlePayload({ settings: { selected_template_id: "tpl-1" } }),
    );

    expect(plan.missingSelectedTemplateId).toBeNull();
  });
});

// Mitschnitt-Fork (F14). The address is the most dangerous field a bundle
// carries, and it used to be the only one nobody could see.
//
// A bundle needs no credential to do harm: readable JSON, no password, the
// shape we describe as safe to hand around. It sets `base_url` to a host of the
// writer's choosing; the importing machine keeps its own key, exactly as
// intended; and from then on that key and the contents of every meeting it
// summarises or transcribes go to whoever wrote the file. The preview said
// "with key / without key" and nothing else.
describe("an imported provider address", () => {
  beforeEach(() => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(storedSettings({}));
    mocks.getSecret.mockResolvedValue({ status: "ok", data: null });
  });

  const hostile = parseBundlePayload({
    providers: { llm: { openai: { base_url: "https://angreifer.tld/v1" } } },
  });

  it("appears in the preview, marked as not the provider's own", async () => {
    const plan = await planImport(hostile);

    expect(plan.providers).toHaveLength(1);
    expect(plan.providers[0]).toMatchObject({
      providerId: "openai",
      baseUrl: "https://angreifer.tld/v1",
      isDefaultEndpoint: false,
    });
  });

  it("names the key already on this machine as one that would travel", async () => {
    mocks.execute.mockResolvedValue([
      {
        id: "ai_provider:llm:openai",
        value_json: JSON.stringify({ base_url: "https://api.openai.com/v1" }),
      },
    ]);
    mocks.getSecret.mockResolvedValue({ status: "ok", data: "sk-des-opfers" });

    const plan = await planImport(hostile);

    expect(plan.providers[0]).toMatchObject({
      carriesKey: false,
      sendsExistingKeyElsewhere: true,
    });
  });

  it("is not written until somebody agrees to it", async () => {
    await expect(applyImport(hostile)).rejects.toBeInstanceOf(
      UnconfirmedEndpointError,
    );
    expect(mocks.setSecret).not.toHaveBeenCalled();
    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  it("is written once somebody has", async () => {
    await expect(
      applyImport(hostile, { confirmedEndpoints: true }),
    ).resolves.toBeUndefined();
  });

  // The ordinary case must not grow a click: a bundle that points providers at
  // their own addresses is the normal one, and a confirmation everybody clicks
  // past is a confirmation nobody reads.
  it("needs no confirmation when it is the provider's own address", async () => {
    const ordinary = parseBundlePayload({
      providers: { llm: { openai: { base_url: "https://api.openai.com/v1" } } },
    });

    const plan = await planImport(ordinary);
    expect(plan.providers[0]).toMatchObject({ isDefaultEndpoint: true });
    await expect(applyImport(ordinary)).resolves.toBeUndefined();
  });

  // A trailing slash is not a different address, and must not be usable to
  // dodge the check in either direction.
  it("is still the provider's own address with a trailing slash", async () => {
    const ordinary = parseBundlePayload({
      providers: {
        llm: { openai: { base_url: "https://api.openai.com/v1/" } },
      },
    });

    expect((await planImport(ordinary)).providers[0]).toMatchObject({
      isDefaultEndpoint: true,
    });
  });
});

// Mitschnitt-Fork (F18). The second, quieter door into the retention deadline.
// The picker asks before it shortens; an import writes the same field with no
// dialog at all, listed as one changed setting among twenty. On a machine that
// already armed its deadline, a shorter imported span would start deleting on
// the next pass -- for a number nobody here ever looked at.
//
// An imported value is a setting, not an answer. So the answer is withdrawn and
// the cleanup pass asks once more, with the count, exactly as it would after a
// restore.
describe("an import that shortens the retention deadline", () => {
  const bundle = (audio_retention: string) =>
    parseBundlePayload({
      settings: { audio_retention },
      providers: {},
      templates: [],
    });

  it("withdraws the answer so the cleanup asks again before it deletes", async () => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(
      storedSettings({
        audio_retention: "oneYear",
        audio_retention_confirmed: true,
      }),
    );

    await applyImport(bundle("thirtyDays"));

    expect(mocks.setSettingValues).toHaveBeenCalledWith({
      audio_retention: "thirtyDays",
      audio_retention_confirmed: false,
    });
  });

  // The control. A longer deadline takes nothing, so there is nothing to ask
  // about and an answer already given stands. Without this the test above
  // would pass on an import that disarms the deadline every single time.
  it("leaves the answer alone when the imported deadline is longer", async () => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(
      storedSettings({
        audio_retention: "thirtyDays",
        audio_retention_confirmed: true,
      }),
    );

    await applyImport(bundle("oneYear"));

    expect(mocks.setSettingValues).toHaveBeenCalledWith({
      audio_retention: "oneYear",
    });
  });

  // A machine that never gave the answer is already in the safe state; writing
  // `false` over it would only add a row that says what the absence says.
  it("writes nothing extra on a machine that never armed its deadline", async () => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(
      storedSettings({ audio_retention: "oneYear" }),
    );

    await applyImport(bundle("thirtyDays"));

    expect(mocks.setSettingValues).toHaveBeenCalledWith({
      audio_retention: "thirtyDays",
    });
  });

  // Found by the second-look review. A machine whose stored deadline is corrupt
  // deletes nothing, because every reader of an unreadable value chooses
  // "forever". Comparing an import against the six-month default instead would
  // call an incoming one-year deadline LONGER, leave the answer armed, and turn
  // "delete nothing" into "delete everything past a year" without a word.
  it("treats an unreadable stored deadline as never, like every other reader", async () => {
    mocks.execute.mockResolvedValue([]);
    mocks.getStoredSettingValues.mockResolvedValue(
      storedSettings({
        audio_retention: "kaputt-halb-geschrieben",
        audio_retention_confirmed: true,
      }),
    );

    await applyImport(bundle("oneYear"));

    expect(mocks.setSettingValues).toHaveBeenCalledWith({
      audio_retention: "oneYear",
      audio_retention_confirmed: false,
    });
  });

  // The answer itself must never travel. It is a statement about ONE machine's
  // own library; carried in a file it would arm a deadline over recordings
  // nobody on the receiving machine has looked at.
  it("cannot be carried in a bundle at all", () => {
    expect(EXPORTED_SETTING_KEYS).not.toContain("audio_retention_confirmed");
    expect(
      parseBundlePayload({
        settings: { audio_retention_confirmed: true },
        providers: {},
        templates: [],
      }).settings,
    ).toEqual({});
  });
});

// Grok-Review 02.09.2026 (Befund 9): die Zustimmungs-Schalter fuer PostHog und
// Sentry sind mit ihren Empfaengern aus dem Schema verschwunden. Aeltere
// Bundles und aeltere app_settings-Zeilen tragen sie weiter. Sie sind
// Altlast, kein Setup -- und duerfen in keine Richtung reisen.
describe("legacy keys of removed receivers", () => {
  const legacyKeys = Object.keys(LEGACY_WITHHELD_SETTING_KEYS);

  // Grok 11 (Review 02.09.2026, C7): genau die zwei -- nicht "mindestens".
  // Ein dritter Eintrag, der hier auftaucht, ist eine Entscheidung, die dieser
  // Test sehen soll, keine, die er durchwinkt.
  it("names exactly the two consent flags that left with PostHog and Sentry", () => {
    expect(legacyKeys).toEqual([
      "telemetry_consent",
      "crash_reporting_consent",
    ]);
  });

  it("keeps them out of the allowlist and out of a bundle read from disk", () => {
    for (const key of legacyKeys) {
      expect(EXPORTED_SETTING_KEYS as readonly string[]).not.toContain(key);
      // Mutant proof 02.09.2026: adding "telemetry_consent" to
      // EXPORTED_SETTING_KEYS turns this red ("expected {} to equal
      // { telemetry_consent: true }") -- the parse path really is the gate.
      expect(
        parseBundlePayload({ settings: { [key]: true } }).settings,
      ).toEqual({});
    }
  });

  it("does not collect them even when the source machine still stores them", async () => {
    mocks.getStoredSettingValues.mockResolvedValue(
      storedSettings({
        ...SOURCE_SETTINGS,
        crash_reporting_consent: true,
      }),
    );

    const payload = await collectBundlePayload({ includeSecrets: false });

    for (const key of legacyKeys) {
      expect(payload.settings).not.toHaveProperty(key);
    }
  });
});
