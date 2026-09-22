import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  connectedImportSyncQueryOptions: vi.fn(),
}));

vi.mock("~/imports/connected-import", () => ({
  connectedImportCredentialsQueryOptions: (providerId: string) => ({
    queryKey: ["credentials", providerId],
  }),
  connectedImportSyncQueryOptions: (
    provider: { id: string },
    enabled: boolean,
  ) => mocks.connectedImportSyncQueryOptions(provider, enabled),
  isLocalConnectedImport: (provider: { directImport?: string }) =>
    provider.directImport === "mcp-oauth" || provider.directImport === "cli",
}));

vi.mock("@tanstack/react-query", () => ({
  useQueries: ({ queries }: { queries: { queryKey: string[] }[] }) =>
    queries[0]?.queryKey[0] === "credentials"
      ? queries.map(() => ({ data: {} }))
      : queries.map(() => ({})),
}));

import { MeetingImportSync } from "./meeting-import-sync";

describe("MeetingImportSync", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.connectedImportSyncQueryOptions.mockImplementation(
      (provider: { id: string }, enabled: boolean) => ({
        queryKey: ["sync", provider.id, String(enabled)],
      }),
    );
  });

  afterEach(cleanup);

  it("enables a connected import once its credentials are cached", () => {
    render(<MeetingImportSync />);

    expect(mocks.connectedImportSyncQueryOptions).toHaveBeenCalled();
    expect(
      mocks.connectedImportSyncQueryOptions.mock.calls.every(
        ([, enabled]) => enabled === true,
      ),
    ).toBe(true);
  });

  it("only syncs local connected providers", () => {
    render(<MeetingImportSync />);

    const syncedIds = mocks.connectedImportSyncQueryOptions.mock.calls.map(
      ([provider]) => provider.id,
    );
    expect(syncedIds).toContain("granola");
    expect(syncedIds).toContain("plaud");
    expect(syncedIds).not.toContain("zoom");
  });
});
