import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  execute,
  executeProxy,
  executeTransaction,
  subscribe,
} from "@anlg/plugin-db";

const { enqueueDatabaseWriteMock } = vi.hoisted(() => ({
  enqueueDatabaseWriteMock: vi.fn(
    (_key: string, operation: () => Promise<unknown>) => operation(),
  ),
}));

vi.mock("~/db/write-queue", () => ({
  enqueueDatabaseWrite: enqueueDatabaseWriteMock,
}));

import {
  getTemplateById,
  useCreateTemplate,
  useDeleteTemplate,
  useUserTemplate,
  useUserTemplates,
} from "./queries";
import { DEFAULT_TEMPLATE_ICON } from "./template-icon";

type SubscribeOptions<T> = {
  onData: (rows: T[]) => void;
  onError?: (message: string) => void;
};

describe("template queries", () => {
  const executeProxyMock = vi.mocked(executeProxy);
  const subscribeMock = vi.mocked(subscribe);

  function createWrapper() {
    const queryClient = new QueryClient({
      defaultOptions: {
        mutations: { retry: false },
        queries: { retry: false },
      },
    });

    return function Wrapper({ children }: { children: ReactNode }) {
      return (
        <QueryClientProvider client={queryClient}>
          {children}
        </QueryClientProvider>
      );
    };
  }

  afterEach(cleanup);

  beforeEach(() => {
    vi.clearAllMocks();
    executeProxyMock.mockResolvedValue({ rows: [] });
    subscribeMock.mockResolvedValue(async () => {});
  });

  it("maps live query rows that use raw SQLite field names", async () => {
    const subscriptions: Array<SubscribeOptions<Record<string, unknown>>> = [];

    subscribeMock.mockImplementation(async (_sql, _params, options) => {
      subscriptions.push(options);
      return async () => {};
    });

    const { result: templatesResult } = renderHook(() => useUserTemplates());
    const { result: templateResult } = renderHook(() =>
      useUserTemplate("template-1"),
    );

    await waitFor(() => {
      expect(subscribeMock).toHaveBeenCalledTimes(2);
    });

    act(() => {
      const rows = [
        {
          id: "template-1",
          title: "Standup",
          description: "Daily sync",
          pinned: true,
          pin_order: 2,
          category: "meetings",
          icon_json: '{"type":"emoji","value":"☀️"}',
          targets_json: '["engineering"]',
          sections_json: '[{"title":"Notes","description":"Capture updates"}]',
          created_at: "2026-04-14T00:00:00Z",
          updated_at: "2026-04-14T00:00:00Z",
        },
      ];

      subscriptions[0]?.onData(rows);
      subscriptions[1]?.onData(rows);
    });

    await waitFor(() => {
      expect(templatesResult.current).toEqual([
        {
          id: "template-1",
          title: "Standup",
          description: "Daily sync",
          pinned: true,
          pinOrder: 2,
          category: "meetings",
          icon: { type: "emoji", value: "☀️" },
          targets: ["engineering"],
          sections: [{ title: "Notes", description: "Capture updates" }],
        },
      ]);
      expect(templateResult.current.data).toEqual({
        id: "template-1",
        title: "Standup",
        description: "Daily sync",
        pinned: true,
        pinOrder: 2,
        category: "meetings",
        icon: { type: "emoji", value: "☀️" },
        targets: ["engineering"],
        sections: [{ title: "Notes", description: "Capture updates" }],
      });
    });
  });

  it("keeps live template rows visible when stored template JSON is invalid", async () => {
    const subscriptions: Array<SubscribeOptions<Record<string, unknown>>> = [];

    subscribeMock.mockImplementation(async (_sql, _params, options) => {
      subscriptions.push(options);
      return async () => {};
    });

    const { result } = renderHook(() => useUserTemplates());

    await waitFor(() => {
      expect(subscribeMock).toHaveBeenCalledTimes(1);
    });

    act(() => {
      subscriptions[0]?.onData([
        {
          id: "template-1",
          title: "Draft Template",
          description: "",
          pinned: false,
          pin_order: null,
          category: null,
          icon_json: "{",
          targets_json: "{",
          sections_json: '[{"title":"","description":""}]',
          created_at: "2026-04-14T00:00:00Z",
          updated_at: "2026-04-14T00:00:00Z",
        },
      ]);
    });

    await waitFor(() => {
      expect(result.current).toEqual([
        {
          id: "template-1",
          title: "Draft Template",
          description: "",
          pinned: false,
          pinOrder: undefined,
          category: undefined,
          icon: DEFAULT_TEMPLATE_ICON,
          targets: undefined,
          sections: [{ title: "", description: "" }],
        },
      ]);
    });
  });

  it("keeps execute-path reads working with Drizzle-mapped rows", async () => {
    executeProxyMock.mockResolvedValue({
      rows: [
        [
          "template-1",
          "Standup",
          "Daily sync",
          0,
          null,
          null,
          '{"type":"icon","value":"target","color":"#5b67d8"}',
          '["engineering"]',
          '[{"title":"Notes","description":"Capture updates"}]',
          "2026-04-14T00:00:00Z",
          "2026-04-14T00:00:00Z",
        ],
      ],
    });

    await expect(getTemplateById("template-1")).resolves.toEqual({
      id: "template-1",
      title: "Standup",
      description: "Daily sync",
      pinned: false,
      pinOrder: undefined,
      category: undefined,
      icon: { type: "icon", value: "target", color: "#5b67d8" },
      targets: ["engineering"],
      sections: [{ title: "Notes", description: "Capture updates" }],
    });
  });

  it("creates a template row through the SQLite proxy", async () => {
    const { result } = renderHook(() => useCreateTemplate(), {
      wrapper: createWrapper(),
    });

    let createdId: string | undefined;
    await act(async () => {
      createdId = await result.current({
        title: "New Template",
        description: "",
        sections: [],
      });
    });

    expect(createdId).toEqual(expect.any(String));
    expect(executeProxyMock).toHaveBeenCalledWith(
      expect.stringContaining('insert into "templates"'),
      [
        createdId,
        "New Template",
        "",
        0,
        JSON.stringify(DEFAULT_TEMPLATE_ICON),
        null,
        "[]",
      ],
      "run",
    );
    expect(executeProxyMock).toHaveBeenCalledWith(
      expect.stringContaining("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')"),
      expect.any(Array),
      "run",
    );
  });

  // F6: selected_template_id zeigt auf eine Vorlage. Wird genau die geloescht,
  // faellt der Standard auf "Auto" ("") zurueck -- sonst behauptet die
  // Einstellung weiter eine Wahl, und useEnhancedNotes laedt eine ID ins Leere.
  //
  // C8 (2/8), Review 02.09.2026: kein Lesen mehr ausserhalb der Warteschlange.
  // Der Vergleich steckt im SQL (WHERE value_json = json_quote(?)), der Schreib-
  // vorgang laeuft in der "app-settings"-Warteschlange -- ein gleichzeitiger
  // Wechsel auf eine andere Vorlage wird so nie ueberschrieben, und eine
  // Loeschung, die die Wahl nicht trifft, fasst updated_at nicht an.
  describe("deleting the selected template", () => {
    const executeMock = vi.mocked(execute);
    const executeTransactionMock = vi.mocked(executeTransaction);

    async function deleteTemplate(id: string) {
      const { result } = renderHook(() => useDeleteTemplate(), {
        wrapper: createWrapper(),
      });
      await act(async () => {
        await result.current(id);
      });
    }

    // D4 (Fix-Runde 1d): DELETE und UPDATE in EINER Transaktion in der
    // Warteschlange -- vorher lief das DELETE als eigener Drizzle-Aufruf
    // davor. Die Datenbank ist hier ein Mock, also pinnt der Test die
    // Statements wortgenau; dass ein Fehler nach dem DELETE beides
    // zuruecknimmt, beweist der Zwilling am echten SQLite in crates/db-app
    // (template_delete_rolls_back_the_delete_when_the_reset_fails).
    it.each(["template-1", "template-2"])(
      "deletes %s and resets selected_template_id to Auto in one transaction inside the write queue",
      async (id) => {
        await deleteTemplate(id);

        expect(executeProxyMock).not.toHaveBeenCalled();
        // Kein Lesen vor dem Schreiben: die Bedingung ist Teil des Statements.
        expect(executeMock).not.toHaveBeenCalled();
        expect(enqueueDatabaseWriteMock).toHaveBeenCalledWith(
          "app-settings",
          expect.any(Function),
        );
        expect(executeTransactionMock).toHaveBeenCalledTimes(1);
        const [remove, reset] = executeTransactionMock.mock.calls[0][0];
        expect(remove.sql.replace(/\s+/g, " ").trim()).toBe(
          "DELETE FROM templates WHERE id = ?",
        );
        expect(remove.params).toEqual([id]);
        expect(reset.sql.replace(/\s+/g, " ").trim()).toBe(
          `UPDATE app_settings SET value_json = '""', updated_at = ? WHERE id = 'selected_template_id' AND value_json = json_quote(?)`,
        );
        expect(reset.params).toEqual([expect.any(String), id]);
      },
    );
  });
});
