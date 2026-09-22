import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  listMeetings: vi.fn(),
  getMeeting: vi.fn(),
  getMeetingTranscript: vi.fn(),
  getRecurringMeetingHistory: vi.fn(),
}));

vi.mock("@anlg/plugin-db", () => mocks);

import {
  buildGetMeetingTool,
  buildGetMeetingTranscriptTool,
  buildGetRecurringMeetingHistoryTool,
  buildListMeetingsTool,
} from "./meetings";

describe("canonical meeting chat tools", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("passes MCP-shaped inputs to the shared meeting service", async () => {
    mocks.listMeetings.mockResolvedValue({ meetings: [], pagination: {} });
    mocks.getMeeting.mockResolvedValue({ id: "meeting-1" });
    mocks.getMeetingTranscript.mockResolvedValue({
      meeting_id: "meeting-1",
      words: [],
      pagination: {},
    });
    mocks.getRecurringMeetingHistory.mockResolvedValue({
      meetings: [],
      pagination: {},
    });

    await (buildListMeetingsTool() as any).execute({
      query: "planning",
      limit: 10,
      offset: 20,
    });
    await (buildGetMeetingTool() as any).execute({
      meeting_id: "meeting-1",
    });
    await (buildGetMeetingTranscriptTool() as any).execute({
      meeting_id: "meeting-1",
      limit: 200,
      offset: 0,
    });
    await (buildGetRecurringMeetingHistoryTool() as any).execute({
      meeting_id: "meeting-1",
      limit: 20,
      offset: 0,
    });

    expect(mocks.listMeetings).toHaveBeenCalledWith({
      query: "planning",
      limit: 10,
      offset: 20,
    });
    expect(mocks.getMeeting).toHaveBeenCalledWith({
      meeting_id: "meeting-1",
    });
    expect(mocks.getMeetingTranscript).toHaveBeenCalledWith({
      meeting_id: "meeting-1",
      limit: 200,
      offset: 0,
    });
    expect(mocks.getRecurringMeetingHistory).toHaveBeenCalledWith({
      meeting_id: "meeting-1",
      limit: 20,
      offset: 0,
    });
  });
});
