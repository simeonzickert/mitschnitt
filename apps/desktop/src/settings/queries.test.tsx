import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  execute: vi.fn(),
  getPreferredLanguages: vi.fn(),
  getTemplateSource: vi.fn(),
  setAutomaticUpdatesEnabled: vi.fn(async () => undefined),
  setRespectDoNotDisturb: vi.fn(async () => undefined),
  executeTransaction: vi.fn(
    (_statements: Array<{ sql: string; params: unknown[] }>) =>
      Promise.resolve([1]),
  ),
  platform: vi.fn(() => "macos"),
  arch: vi.fn(() => "aarch64"),
}));

vi.mock("@tauri-apps/plugin-os", () => ({
  platform: mocks.platform,
  arch: mocks.arch,
}));

vi.mock("@anlg/plugin-detect", () => ({
  commands: {
    getPreferredLanguages: mocks.getPreferredLanguages,
    setRespectDoNotDisturb: mocks.setRespectDoNotDisturb,
  },
}));

vi.mock("@anlg/plugin-template", () => ({
  commands: {
    getTemplateSource: mocks.getTemplateSource,
  },
}));

vi.mock("@anlg/plugin-updater2", () => ({
  commands: {
    setAutomaticUpdatesEnabled: mocks.setAutomaticUpdatesEnabled,
  },
}));

vi.mock("~/db", () => ({
  executeTransaction: mocks.executeTransaction,
  liveQueryClient: { execute: mocks.execute },
  useLiveQuery: vi.fn(() => ({ data: undefined })),
}));

vi.mock("~/db/write-queue", () => ({
  enqueueDatabaseWrite: (_key: string, operation: () => Promise<unknown>) =>
    operation(),
}));

import {
  initializeApplicationSettings,
  parseSettingRows,
  setSettingValues,
  updateSettingValue,
} from "./queries";

// Mitschnitt-Fork (F17). `initializeApplicationSettings` now also settles how
// long this machine keeps recordings and writes the answer down, because the
// cleanup pass refuses to delete on an unwritten one. Tests about other
// migrations seed an answer so that decision stays out of their transaction.
const RETENTION_ANSWERED = {
  id: "audio_retention",
  value_json: JSON.stringify("forever"),
};

describe("SQLite settings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.platform.mockReturnValue("macos");
    mocks.arch.mockReturnValue("aarch64");
    mocks.execute.mockResolvedValue([]);
    mocks.getPreferredLanguages.mockResolvedValue({
      status: "error",
      error: "unavailable",
    });
    mocks.getTemplateSource.mockResolvedValue({
      status: "ok",
      data: "- Use Markdown.",
    });
  });

  it("maps the imported settings document into typed values", () => {
    const result = parseSettingRows([
      {
        id: "legacy_settings_document",
        value_json: JSON.stringify({
          general: {
            theme: "dark",
            save_recordings: false,
          },
          language: {
            spoken_languages: ["en", "ko"],
          },
          notification: {
            ignored_platforms: ["com.example.video"],
          },
        }),
      },
    ]);

    expect(result.values.theme).toBe("dark");
    expect(result.values.audio_retention).toBe("none");
    expect(result.values.spoken_languages).toBe('["en","ko"]');
    expect(result.values.ignored_platforms).toBe('["com.example.video"]');
    expect(result.hasValues.has("theme")).toBe(true);
  });

  it("prefers valid direct rows and falls back from corrupt ones", () => {
    const result = parseSettingRows([
      {
        id: "legacy_settings_document",
        value_json: JSON.stringify({
          general: { theme: "dark", week_start: "monday" },
        }),
      },
      { id: "theme", value_json: JSON.stringify("light") },
      { id: "week_start", value_json: "not-json" },
    ]);

    expect(result.values.theme).toBe("light");
    expect(result.values.week_start).toBe("monday");
  });

  it("recovers main-store values after settings document values and aliases", () => {
    const result = parseSettingRows([
      {
        id: "legacy_settings_document",
        value_json: JSON.stringify({
          general: { ai_language: "fr" },
          language: { spoken_languages: ["fr"] },
        }),
      },
      {
        id: "legacy_main_values_document",
        value_json: JSON.stringify({
          ai_language: "ko",
          spoken_languages: JSON.stringify(["ko"]),
          theme: "dark",
        }),
      },
    ]);

    expect(result.values.ai_language).toBe("fr");
    expect(result.values.spoken_languages).toBe('["fr"]');
    expect(result.values.theme).toBe("dark");
  });

  it("writes multiple independent values in one transaction", async () => {
    await setSettingValues({
      theme: "dark",
      notification_event: false,
    });

    const statements = mocks.executeTransaction.mock.calls[0][0];
    expect(statements).toHaveLength(2);
    // Every setting goes to the one surviving table. A statement that named a
    // table the cloud migration dropped would fail at prepare time, so assert
    // on the table each write actually targets.
    const tablesWritten = statements.map(
      (statement: { sql: string }) =>
        /INSERT INTO (\w+)/.exec(statement.sql)?.[1],
    );
    expect(tablesWritten).toEqual(["app_settings", "app_settings"]);
    expect(statements[0].sql).toContain("ON CONFLICT(id) DO UPDATE");
    expect(statements[0].params.slice(0, 2)).toEqual([
      "theme",
      JSON.stringify("dark"),
    ]);
    expect(statements[1].params.slice(0, 2)).toEqual([
      "notification_event",
      JSON.stringify(false),
    ]);
  });

  it("prefers the last row when a key appears twice", () => {
    const result = parseSettingRows([
      { id: "theme", value_json: JSON.stringify("light") },
      { id: "theme", value_json: JSON.stringify("dark") },
    ]);

    expect(result.values.theme).toBe("dark");
  });

  it("applies the automatic update policy when it changes", async () => {
    await setSettingValues({ automatic_updates: false });

    expect(mocks.setAutomaticUpdatesEnabled).toHaveBeenCalledWith(false);
  });

  it("migrates and persists the consent chat auto-send setting", async () => {
    const imported = parseSettingRows([
      {
        id: "legacy_settings_document",
        value_json: JSON.stringify({
          general: { consent_auto_send_chat: true },
        }),
      },
    ]);

    expect(imported.values.consent_auto_send_chat).toBe(true);

    await setSettingValues({ consent_auto_send_chat: false });

    const statement = mocks.executeTransaction.mock.calls[0][0][0];
    expect(statement.params.slice(0, 2)).toEqual([
      "consent_auto_send_chat",
      JSON.stringify(false),
    ]);
  });

  it("persists the selected microphone in SQLite settings", async () => {
    await setSettingValues({ microphone_device: "External Microphone" });

    const statement = mocks.executeTransaction.mock.calls[0][0][0];
    expect(statement.params.slice(0, 2)).toEqual([
      "microphone_device",
      JSON.stringify("External Microphone"),
    ]);
  });

  it("persists the selected speakers in SQLite settings", async () => {
    await setSettingValues({ speaker_device: "External Speakers" });

    const statement = mocks.executeTransaction.mock.calls[0][0][0];
    expect(statement.params.slice(0, 2)).toEqual([
      "speaker_device",
      JSON.stringify("External Speakers"),
    ]);
  });

  it("persists and restores the summary length mode", async () => {
    await setSettingValues({ summary_length: "crisp" });

    const statement = mocks.executeTransaction.mock.calls[0][0][0];
    expect(statement.params.slice(0, 2)).toEqual([
      "summary_length",
      JSON.stringify("crisp"),
    ]);

    const restored = parseSettingRows([
      { id: "summary_length", value_json: JSON.stringify("balanced") },
    ]);
    expect(restored.values.summary_length).toBe("balanced");
    expect(restored.hasValues.has("summary_length")).toBe(true);
  });

  it("initializes languages from OS preferences", async () => {
    let rows: Array<{ id: string; value_json: string }> = [RETENTION_ANSWERED];
    mocks.execute.mockImplementation(async () => rows);
    mocks.executeTransaction.mockImplementation(async (statements) => {
      rows = statements.map((statement) => ({
        id: String(statement.params[0]),
        value_json: String(statement.params[1]),
      }));
      return statements.map(() => 1);
    });
    mocks.getPreferredLanguages.mockResolvedValue({
      status: "ok",
      data: ["ko", "en"],
    });

    await initializeApplicationSettings();

    const statements = mocks.executeTransaction.mock.calls[0][0];
    expect(statements.map((statement) => statement.params.slice(0, 2))).toEqual(
      [
        ["ai_language", JSON.stringify("ko")],
        ["spoken_languages", JSON.stringify(JSON.stringify(["en"]))],
      ],
    );
  });

  it("normalizes spoken languages previously populated from the OS", async () => {
    mocks.execute.mockResolvedValue([
      RETENTION_ANSWERED,
      { id: "ai_language", value_json: JSON.stringify("ko") },
      {
        id: "spoken_languages",
        value_json: JSON.stringify(JSON.stringify(["ko", "en"])),
      },
    ]);
    mocks.getPreferredLanguages.mockResolvedValue({
      status: "ok",
      data: ["ko", "en"],
    });

    await initializeApplicationSettings();

    const statements = mocks.executeTransaction.mock.calls[0][0];
    expect(statements.map((statement) => statement.params.slice(0, 2))).toEqual(
      [["spoken_languages", JSON.stringify(JSON.stringify(["en"]))]],
    );
  });

  it("preserves an explicitly empty spoken language list", async () => {
    mocks.execute.mockResolvedValue([
      RETENTION_ANSWERED,
      { id: "ai_language", value_json: JSON.stringify("ko") },
      {
        id: "spoken_languages",
        value_json: JSON.stringify(JSON.stringify([])),
      },
    ]);
    mocks.getPreferredLanguages.mockResolvedValue({
      status: "ok",
      data: ["ko", "en"],
    });

    await initializeApplicationSettings();

    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  it("repairs a selected external transcription provider with no model", async () => {
    let rows = [
      RETENTION_ANSWERED,
      {
        id: "current_stt_provider",
        value_json: JSON.stringify("deepgram"),
      },
      { id: "current_stt_model", value_json: JSON.stringify("") },
    ];
    mocks.execute.mockImplementation(async () => rows);
    mocks.executeTransaction.mockImplementation(async (statements) => {
      rows = statements.map((statement) => ({
        id: String(statement.params[0]),
        value_json: String(statement.params[1]),
      }));
      return statements.map(() => 1);
    });

    await initializeApplicationSettings();

    const statements = mocks.executeTransaction.mock.calls[0][0];
    expect(statements.map((statement) => statement.params.slice(0, 2))).toEqual(
      [["current_stt_model", JSON.stringify("nova-3-general")]],
    );
  });

  it.each([
    ["soniqo-parakeet-streaming", "soniqo"],
    ["apple-speech", "apple_speech"],
  ])(
    "moves the legacy Anarlog %s selection to its on-device provider",
    async (model, provider) => {
      let rows = [
        RETENTION_ANSWERED,
        {
          id: "current_stt_provider",
          value_json: JSON.stringify("anarlog"),
        },
        { id: "current_stt_model", value_json: JSON.stringify(model) },
      ];
      mocks.execute.mockImplementation(async () => rows);
      mocks.executeTransaction.mockImplementation(async (statements) => {
        for (const statement of statements) {
          const id = String(statement.params[0]);
          const next = {
            id,
            value_json: String(statement.params[1]),
          };
          const index = rows.findIndex((row) => row.id === id);
          if (index === -1) rows.push(next);
          else rows[index] = next;
        }
        return statements.map(() => 1);
      });

      await initializeApplicationSettings();

      const statements = mocks.executeTransaction.mock.calls[0][0];
      expect(
        statements.map((statement) => statement.params.slice(0, 2)),
      ).toEqual([["current_stt_provider", JSON.stringify(provider)]]);
    },
  );

  // Kimi/Grok-Review 02.09.2026 (F7). Gespeicherte Kennungen der Gegenstelle
  // des Originals duerfen nicht in einen Client mit leerer Basis-URL laufen.
  // Die eine Stelle, die Einstellungen beim Start geradezieht, raeumt sie weg:
  // STT auf den lokalen Standard, LLM leer -- der Hinweis-Banner sagt dann,
  // dass ein Anbieter zu waehlen ist.
  it.each(["anarlog", "hyprnote"])(
    "moves a stored %s cloud transcription to the local default",
    async (legacyProvider) => {
      let rows = [
        RETENTION_ANSWERED,
        {
          id: "current_stt_provider",
          value_json: JSON.stringify(legacyProvider),
        },
        { id: "current_stt_model", value_json: JSON.stringify("cloud") },
      ];
      mocks.execute.mockImplementation(async () => rows);
      mocks.executeTransaction.mockImplementation(async (statements) => {
        for (const statement of statements) {
          const id = String(statement.params[0]);
          const next = { id, value_json: String(statement.params[1]) };
          const index = rows.findIndex((row) => row.id === id);
          if (index === -1) rows.push(next);
          else rows[index] = next;
        }
        return statements.map(() => 1);
      });

      await initializeApplicationSettings();

      expect(mocks.executeTransaction).toHaveBeenCalled();
      const statements = mocks.executeTransaction.mock.calls[0][0];
      expect(
        statements.map((statement) => statement.params.slice(0, 2)),
      ).toEqual([
        ["current_stt_provider", JSON.stringify("soniqo")],
        ["current_stt_model", JSON.stringify("soniqo-parakeet-batch")],
      ]);
    },
  );

  // Forge 2 (Review 02.09.2026, C3 i). Zwischen dem Lesen am Anfang und dem
  // Schreiben am Ende liegen mehrere awaits (Sprach-Erkennung, Vorlage,
  // Aufbewahrung). Waehlt der Nutzer in der Zeit einen Anbieter, darf der
  // Umzug seine Wahl nicht ueberschreiben: geschrieben wird nur, wenn beim
  // Schreiben noch der alte Wert steht. Vor dem Umbau stand danach wieder
  // soniqo in der Datenbank.
  it("does not overwrite a provider chosen while the start-up tasks were running", async () => {
    const legacy = [
      RETENTION_ANSWERED,
      { id: "current_stt_provider", value_json: JSON.stringify("anarlog") },
      { id: "current_stt_model", value_json: JSON.stringify("cloud") },
    ];
    const chosen = [
      RETENTION_ANSWERED,
      { id: "current_stt_provider", value_json: JSON.stringify("deepgram") },
      {
        id: "current_stt_model",
        value_json: JSON.stringify("nova-3-general"),
      },
    ];
    let reads = 0;
    mocks.execute.mockImplementation(async () =>
      reads++ === 0 ? legacy : chosen,
    );

    await initializeApplicationSettings();

    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  // D3 (Fix-Runde 1d). Der Guard deckte nur die Umzuege; das Standard-Modell
  // und die Sprach-Zeilen kamen aus der ERSTEN Lesung und wurden immer
  // geschrieben. Gespeichert: deepgram ohne Modell. Waehrend der Init
  // waehlt der Nutzer soniox/stt-rt-v5 -- vorher stand danach
  // nova-3-general (Deepgrams Standard) unter dem Soniox-Anbieter.
  it("does not write the old provider's default model over a selection made during start-up", async () => {
    const first = [
      RETENTION_ANSWERED,
      { id: "current_stt_provider", value_json: JSON.stringify("deepgram") },
      { id: "current_stt_model", value_json: JSON.stringify("") },
    ];
    const chosen = [
      RETENTION_ANSWERED,
      { id: "current_stt_provider", value_json: JSON.stringify("soniox") },
      { id: "current_stt_model", value_json: JSON.stringify("stt-rt-v5") },
    ];
    let reads = 0;
    mocks.execute.mockImplementation(async () =>
      reads++ === 0 ? first : chosen,
    );

    await initializeApplicationSettings();

    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  it("does not overwrite a language chosen during start-up with the system language", async () => {
    mocks.getPreferredLanguages.mockResolvedValue({
      status: "ok",
      data: ["en", "de"],
    });
    const first = [RETENTION_ANSWERED];
    const chosen = [
      RETENTION_ANSWERED,
      { id: "ai_language", value_json: JSON.stringify("fr") },
      { id: "spoken_languages", value_json: JSON.stringify(["fr"]) },
    ];
    let reads = 0;
    mocks.execute.mockImplementation(async () =>
      reads++ === 0 ? first : chosen,
    );

    await initializeApplicationSettings();

    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  // Nichts geschrieben heisst nicht "nichts passiert": die Nebenwirkungen
  // (Autostart, Nicht-stoeren, Tray) liefen vorher auf der ersten
  // Momentaufnahme -- ein waehrend der Init umgelegter Schalter wurde so
  // zurueckgelegt. Jetzt laufen sie auf der Neu-Lesung aus der Warteschlange.
  it("applies side effects from the latest rows when nothing had to be written", async () => {
    const first = [RETENTION_ANSWERED];
    const flipped = [
      RETENTION_ANSWERED,
      { id: "respect_dnd", value_json: JSON.stringify(true) },
    ];
    let reads = 0;
    mocks.execute.mockImplementation(async () =>
      reads++ === 0 ? first : flipped,
    );

    await initializeApplicationSettings();

    expect(mocks.executeTransaction).not.toHaveBeenCalled();
    expect(mocks.setRespectDoNotDisturb).toHaveBeenCalledWith(true);
  });

  // D5 (Fix-Runde 1d). Bis hierher galt das Leeren nur fuer ein Paar, das
  // gerade umzieht (`moves &&`); ein bereits lokales Paar -- soniqo oder eine
  // Modelldatei -- lief auf Windows/Linux/Intel-Mac unveraendert durch, als
  // eingerichtet, und verband nie. Jetzt dieselbe Funktion wie an der Kante.
  it.each([
    ["windows", "x86_64", "soniqo", "soniqo-parakeet-batch"],
    ["linux", "x86_64", "soniqo", "soniqo-parakeet-batch"],
    ["macos", "x86_64", "soniqo", "soniqo-parakeet-batch"],
    ["windows", "x86_64", "local_file", "local-file"],
    ["linux", "x86_64", "local_file", "local-file"],
    ["macos", "x86_64", "local_file", "local-file"],
  ])(
    "clears a stored local pair on %s/%s (%s / %s) that cannot run there",
    async (currentPlatform, currentArch, provider, model) => {
      mocks.platform.mockReturnValue(currentPlatform);
      mocks.arch.mockReturnValue(currentArch);
      let rows = [
        RETENTION_ANSWERED,
        { id: "current_stt_provider", value_json: JSON.stringify(provider) },
        { id: "current_stt_model", value_json: JSON.stringify(model) },
      ];
      mocks.execute.mockImplementation(async () => rows);
      mocks.executeTransaction.mockImplementation(async (statements) => {
        for (const statement of statements) {
          const id = String(statement.params[0]);
          const next = { id, value_json: String(statement.params[1]) };
          const index = rows.findIndex((row) => row.id === id);
          if (index === -1) rows.push(next);
          else rows[index] = next;
        }
        return statements.map(() => 1);
      });

      await initializeApplicationSettings();

      const statements = mocks.executeTransaction.mock.calls[0][0];
      expect(
        statements.map((statement) => statement.params.slice(0, 2)),
      ).toEqual([
        ["current_stt_provider", JSON.stringify("")],
        ["current_stt_model", JSON.stringify("")],
      ]);
    },
  );

  it("keeps a stored local pair on Apple Silicon", async () => {
    mocks.execute.mockResolvedValue([
      RETENTION_ANSWERED,
      { id: "current_stt_provider", value_json: JSON.stringify("soniqo") },
      {
        id: "current_stt_model",
        value_json: JSON.stringify("soniqo-parakeet-batch"),
      },
    ]);

    await initializeApplicationSettings();

    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  // Opus 5 (Review 02.09.2026, C3 iv). Der lokale Standard ist ein
  // Apple-Silicon-Modell. Auf einer Plattform ohne lokale Transkription
  // wuerde der Umzug ein Paar schreiben, das isConfiguredSttModel fuer
  // eingerichtet haelt und das nie laeuft -- kein Banner, keine
  // Transkription. Dort bleibt die Wahl leer, und der Banner fragt.
  it.each([
    ["windows", "x86_64"],
    ["linux", "x86_64"],
    ["macos", "x86_64"],
  ])(
    "writes an empty selection instead of the local default on %s/%s",
    async (currentPlatform, currentArch) => {
      mocks.platform.mockReturnValue(currentPlatform);
      mocks.arch.mockReturnValue(currentArch);
      let rows = [
        RETENTION_ANSWERED,
        { id: "current_stt_provider", value_json: JSON.stringify("anarlog") },
        { id: "current_stt_model", value_json: JSON.stringify("cloud") },
      ];
      mocks.execute.mockImplementation(async () => rows);
      mocks.executeTransaction.mockImplementation(async (statements) => {
        for (const statement of statements) {
          const id = String(statement.params[0]);
          const next = { id, value_json: String(statement.params[1]) };
          const index = rows.findIndex((row) => row.id === id);
          if (index === -1) rows.push(next);
          else rows[index] = next;
        }
        return statements.map(() => 1);
      });

      await initializeApplicationSettings();

      const statements = mocks.executeTransaction.mock.calls[0][0];
      expect(
        statements.map((statement) => statement.params.slice(0, 2)),
      ).toEqual([
        ["current_stt_provider", JSON.stringify("")],
        ["current_stt_model", JSON.stringify("")],
      ]);
    },
  );

  it.each(["anarlog", "hyprnote"])(
    "clears a stored %s summary provider so the banner asks for a new one",
    async (legacyProvider) => {
      let rows = [
        RETENTION_ANSWERED,
        {
          id: "current_llm_provider",
          value_json: JSON.stringify(legacyProvider),
        },
        { id: "current_llm_model", value_json: JSON.stringify("Auto") },
      ];
      mocks.execute.mockImplementation(async () => rows);
      mocks.executeTransaction.mockImplementation(async (statements) => {
        for (const statement of statements) {
          const id = String(statement.params[0]);
          const next = { id, value_json: String(statement.params[1]) };
          const index = rows.findIndex((row) => row.id === id);
          if (index === -1) rows.push(next);
          else rows[index] = next;
        }
        return statements.map(() => 1);
      });

      await initializeApplicationSettings();

      expect(mocks.executeTransaction).toHaveBeenCalled();
      const statements = mocks.executeTransaction.mock.calls[0][0];
      expect(
        statements.map((statement) => statement.params.slice(0, 2)),
      ).toEqual([
        ["current_llm_provider", JSON.stringify("")],
        ["current_llm_model", JSON.stringify("")],
      ]);
    },
  );

  // Grok 11 (Review 02.09.2026, C7): die zwei Umzuege sind unabhaengig. Ein
  // gueltiges STT-Paar neben einem Alt-LLM-Wert bleibt stehen -- geschrieben
  // wird nur, was umzieht.
  it.each([
    ["soniqo", "soniqo-parakeet-batch"],
    ["openrouter", "openai/gpt-transcribe"],
  ])(
    "touches only the legacy LLM value next to a valid %s / %s selection",
    async (sttProvider, sttModel) => {
      mocks.execute.mockResolvedValue([
        RETENTION_ANSWERED,
        {
          id: "current_stt_provider",
          value_json: JSON.stringify(sttProvider),
        },
        { id: "current_stt_model", value_json: JSON.stringify(sttModel) },
        { id: "current_llm_provider", value_json: JSON.stringify("anarlog") },
        { id: "current_llm_model", value_json: JSON.stringify("Auto") },
      ]);

      await initializeApplicationSettings();

      const statements = mocks.executeTransaction.mock.calls[0][0];
      expect(
        statements.map((statement) => statement.params.slice(0, 2)),
      ).toEqual([
        ["current_llm_provider", JSON.stringify("")],
        ["current_llm_model", JSON.stringify("")],
      ]);
    },
  );

  it("leaves a chosen summary provider untouched", async () => {
    mocks.execute.mockResolvedValue([
      RETENTION_ANSWERED,
      { id: "current_llm_provider", value_json: JSON.stringify("openrouter") },
      {
        id: "current_llm_model",
        value_json: JSON.stringify("openai/gpt-5.6-terra"),
      },
    ]);

    await initializeApplicationSettings();

    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  it("migrates legacy summary instructions into the editable Auto format", async () => {
    let rows = [
      {
        id: "custom_summary_instructions",
        value_json: JSON.stringify(
          "Use the selected summary template for the summary structure and section headings.\n\nStart with decisions.\n\n{{ template }}",
        ),
      },
    ];
    mocks.execute.mockImplementation(async () => rows);
    mocks.executeTransaction.mockImplementation(async (statements) => {
      for (const statement of statements) {
        const id = String(statement.params[0]);
        const next = {
          id,
          value_json: String(statement.params[1]),
        };
        const index = rows.findIndex((row) => row.id === id);
        if (index === -1) rows.push(next);
        else rows[index] = next;
      }
      return statements.map(() => 1);
    });

    await initializeApplicationSettings();

    expect(mocks.getTemplateSource).toHaveBeenCalledWith("enhanceFormat");
    const statement = mocks.executeTransaction.mock.calls[0][0][0];
    expect(statement.params[0]).toBe("auto_summary_prompt");
    expect(JSON.parse(String(statement.params[1]))).toBe(`- Use Markdown.

Start with decisions.`);
  });

  it("does not overwrite an explicitly reset Auto format", async () => {
    mocks.execute.mockResolvedValue([
      RETENTION_ANSWERED,
      {
        id: "custom_summary_instructions",
        value_json: JSON.stringify("Start with decisions."),
      },
      { id: "auto_summary_prompt", value_json: JSON.stringify("") },
    ]);

    await initializeApplicationSettings();

    expect(mocks.getTemplateSource).not.toHaveBeenCalled();
    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  it.each([
    "",
    "Use the selected summary template for the summary structure and section headings.\n\n{{ template }}",
  ])(
    "ignores legacy default-only summary instructions",
    async (instructions) => {
      mocks.execute.mockResolvedValue([
        RETENTION_ANSWERED,
        {
          id: "custom_summary_instructions",
          value_json: JSON.stringify(instructions),
        },
      ]);

      await initializeApplicationSettings();

      expect(mocks.getTemplateSource).not.toHaveBeenCalled();
      expect(mocks.executeTransaction).not.toHaveBeenCalled();
    },
  );

  it("keeps legacy Jinja-like text literal in the migrated prompt", async () => {
    mocks.execute.mockResolvedValue([
      {
        id: "custom_summary_instructions",
        value_json: JSON.stringify("Group by {{ customer_name }}."),
      },
    ]);

    await initializeApplicationSettings();

    const statement = mocks.executeTransaction.mock.calls[0][0][0];
    expect(JSON.parse(String(statement.params[1]))).toContain(
      'Group by {{ "{{" }} customer_name }}.',
    );
  });

  it("leaves legacy instructions untouched when the prompt source is unavailable", async () => {
    mocks.execute.mockResolvedValue([
      RETENTION_ANSWERED,
      {
        id: "custom_summary_instructions",
        value_json: JSON.stringify("Start with decisions."),
      },
    ]);
    mocks.getTemplateSource.mockRejectedValue(new Error("plugin unavailable"));

    await expect(initializeApplicationSettings()).resolves.toBeUndefined();

    expect(mocks.executeTransaction).not.toHaveBeenCalled();
  });

  it("updates against the latest SQLite value inside the write queue", async () => {
    mocks.execute.mockResolvedValue([
      {
        id: "personalization_dictionary_terms",
        value_json: JSON.stringify(JSON.stringify(["Anarlog"])),
      },
    ]);

    const next = await updateSettingValue(
      "personalization_dictionary_terms",
      (current) => JSON.stringify([...JSON.parse(current ?? "[]"), "Erebor"]),
    );

    expect(next).toBe(JSON.stringify(["Anarlog", "Erebor"]));
    const statement = mocks.executeTransaction.mock.calls[0][0][0];
    expect(statement.params.slice(0, 2)).toEqual([
      "personalization_dictionary_terms",
      JSON.stringify(next),
    ]);
  });
});

// Mitschnitt-Fork (F17). The retention deadline now deletes the recording from
// the meeting folder as well as from the machine room, and six months is the
// value a machine that has never been asked falls back to. Applying that to a
// library that is already there would take a year of meetings on the first
// cleanup pass after the update, and nobody said yes to it. So the first start
// writes the answer down -- and what it writes depends on whether there is
// anything to lose.
describe("settling how long recordings are kept", () => {
  // Its own reset: this block sits outside the suite above and would otherwise
  // read that suite's leftover calls as its own.
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.execute.mockResolvedValue([]);
    mocks.executeTransaction.mockImplementation(async (statements) =>
      statements.map(() => 1),
    );
    mocks.getPreferredLanguages.mockResolvedValue({
      status: "error",
      error: "unavailable",
    });
    mocks.getTemplateSource.mockResolvedValue({
      status: "ok",
      data: "- Use Markdown.",
    });
  });

  function seedSettings(
    rows: Array<{ id: string; value_json: string }>,
    holdsMeetings: boolean,
  ) {
    mocks.execute.mockImplementation(async (sql: string) =>
      sql.includes("any_session")
        ? [{ any_session: holdsMeetings ? 1 : 0 }]
        : rows,
    );
  }

  function writtenValue(key: string): unknown {
    for (const [statements] of mocks.executeTransaction.mock.calls) {
      for (const statement of statements) {
        if (String(statement.params[0]) === key) {
          return JSON.parse(String(statement.params[1]));
        }
      }
    }
    return undefined;
  }

  it("writes 'never delete' on a machine that already holds meetings", async () => {
    seedSettings([], true);

    await initializeApplicationSettings();

    expect(writtenValue("audio_retention")).toBe("forever");
  });

  it("writes the six-month default on a machine that holds nothing", async () => {
    seedSettings([], false);

    await initializeApplicationSettings();

    expect(writtenValue("audio_retention")).toBe("sixMonths");
  });

  it("leaves a choice somebody actually made alone", async () => {
    seedSettings(
      [{ id: "audio_retention", value_json: JSON.stringify("thirtyDays") }],
      true,
    );

    await initializeApplicationSettings();

    expect(writtenValue("audio_retention")).toBeUndefined();
  });

  // The legacy switch was a choice too: "don't save recordings" is an answer,
  // and reading it as "never asked" would turn it into six months of keeping.
  it("leaves the legacy save-recordings switch alone", async () => {
    seedSettings(
      [
        {
          id: "legacy_settings_document",
          value_json: JSON.stringify({ general: { save_recordings: false } }),
        },
      ],
      true,
    );

    await initializeApplicationSettings();

    expect(writtenValue("audio_retention")).toBeUndefined();
  });

  // The switch that predates the deadline. A machine carrying only
  // `save_recordings: false` -- from a settings bundle exported before the
  // deadline existed -- has said "don't keep recordings", and turning that into
  // six months of keeping would be just as much of an unasked-for change as the
  // other direction.
  it("writes down the old don't-save switch as the answer it already was", async () => {
    seedSettings(
      [{ id: "save_recordings", value_json: JSON.stringify(false) }],
      true,
    );

    await initializeApplicationSettings();

    expect(writtenValue("audio_retention")).toBe("none");
  });

  // A database that cannot be counted must not be guessed at in either
  // direction: nothing is written, the cleanup stays refused, and the next start
  // asks again.
  it("writes nothing when the meetings cannot be counted", async () => {
    mocks.execute.mockImplementation(async (sql: string) => {
      if (sql.includes("any_session")) throw new Error("database is locked");
      return [];
    });

    await expect(initializeApplicationSettings()).resolves.toBeUndefined();

    expect(writtenValue("audio_retention")).toBeUndefined();
  });
});
