import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { config, startServerForPathMock, os } = vi.hoisted(() => ({
  config: {
    current_stt_provider: "anarlog",
    current_stt_model: "cloud",
    local_stt_model_path: "",
  },
  startServerForPathMock: vi.fn(),
  os: { platform: "macos", arch: "aarch64" },
}));

vi.mock("@tauri-apps/plugin-os", () => ({
  platform: () => os.platform,
  arch: () => os.arch,
}));

vi.mock("@anlg/plugin-local-stt", () => ({
  commands: {
    getServerForModel: vi.fn(),
    isModelDownloaded: vi.fn(async () => ({ status: "ok", data: false })),
    startServerForPath: startServerForPathMock,
  },
}));

vi.mock("~/settings/providers", () => ({
  useAiProvider: () => ({
    type: "stt",
    base_url: "   ",
    api_key: "",
  }),
}));

vi.mock("~/shared/config", () => ({
  useConfigValues: () => config,
}));

// D5 (Fix-Runde 1d): the real capabilities module, so the platform rule
// below is the one the start-up migration uses too, not a stand-in.
vi.mock("~/stt/capabilities", async (importOriginal) => ({
  ...(await importOriginal<typeof import("~/stt/capabilities")>()),
  isRealtimeLocalModel: () => false,
}));

import { useSTTConnection } from "./useSTTConnection";

describe("useSTTConnection", () => {
  beforeEach(() => {
    config.current_stt_provider = "anarlog";
    config.current_stt_model = "cloud";
    config.local_stt_model_path = "";
    os.platform = "macos";
    os.arch = "aarch64";
    startServerForPathMock.mockReset();
  });

  // D5 (Fix-Runde 1d). A local pair on a platform without local
  // transcription used to count as a local model here: isLocalModel true,
  // the download query running against a server that cannot exist, no
  // banner. Now the pair is not set -- conn null, isLocalModel false -- and
  // the banner asks for a provider; a model file is not started either.
  it.each([
    ["windows", "x86_64", "soniqo", "soniqo-parakeet-batch"],
    ["linux", "x86_64", "soniqo", "soniqo-parakeet-batch"],
    ["macos", "x86_64", "soniqo", "soniqo-parakeet-batch"],
    ["windows", "x86_64", "local_file", "local-file"],
    ["linux", "x86_64", "local_file", "local-file"],
    ["macos", "x86_64", "local_file", "local-file"],
  ])(
    "treats a stored local pair on %s/%s (%s / %s) as not set",
    async (currentPlatform, currentArch, provider, model) => {
      os.platform = currentPlatform;
      os.arch = currentArch;
      config.current_stt_provider = provider;
      config.current_stt_model = model;
      config.local_stt_model_path = "/models/ggml-small.bin";
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      const wrapper = ({ children }: { children: ReactNode }) =>
        createElement(QueryClientProvider, { client: queryClient }, children);

      const { result } = renderHook(() => useSTTConnection(), { wrapper });

      expect(result.current.isLocalModel).toBe(false);
      expect(result.current.conn).toBeNull();
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(result.current.local.data).toBeUndefined();
      expect(startServerForPathMock).not.toHaveBeenCalled();
    },
  );

  // Grok 5b (Review 02.09.2026, C3 vi). Die Kante, an der aus den
  // gespeicherten Einstellungen die Verbindung fuer Live und Batch entsteht,
  // zieht ein Alt-Paar der Gegenstelle selbst um -- so erreicht es nie den
  // `removed:`-Arm in Rust, auch nicht zwischen App-Start und Umzug. Vorher
  // meldete der Hook "Cloud-Modell, keine Verbindung"; jetzt ist das Paar
  // der lokale Standard, und die Verbindung haengt nur noch am Download.
  it.each([
    ["anarlog", "cloud"],
    ["hyprnote", "cloud"],
  ])(
    "resolves the retired hosted selection %s / %s to the local default",
    async (provider, model) => {
      config.current_stt_provider = provider;
      config.current_stt_model = model;
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      const wrapper = ({ children }: { children: ReactNode }) =>
        createElement(QueryClientProvider, { client: queryClient }, children);

      const { result } = renderHook(() => useSTTConnection(), { wrapper });

      expect(result.current.isLocalModel).toBe(true);
      await waitFor(() =>
        expect(result.current.local.data?.status).toBe("not_downloaded"),
      );
      expect(result.current.conn).toBeNull();
    },
  );

  it("starts a selected local model file and exposes its local URL", async () => {
    config.current_stt_provider = "local_file";
    config.current_stt_model = "local-file";
    config.local_stt_model_path = "/models/ggml-small.bin";
    startServerForPathMock.mockResolvedValue({
      status: "ok",
      data: "http://127.0.0.1:4040/v1",
    });
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const wrapper = ({ children }: { children: ReactNode }) =>
      createElement(QueryClientProvider, { client: queryClient }, children);

    const { result } = renderHook(() => useSTTConnection(), { wrapper });

    await waitFor(() =>
      expect(result.current.conn).toEqual({
        provider: "local_file",
        model: "local-file",
        baseUrl: "http://127.0.0.1:4040/v1",
        apiKey: "",
      }),
    );
    expect(startServerForPathMock).toHaveBeenCalledWith(
      "/models/ggml-small.bin",
    );
  });
});
