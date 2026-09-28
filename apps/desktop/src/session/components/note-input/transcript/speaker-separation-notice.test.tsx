import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { TranscriptSpeakerSeparationNotice } from "./speaker-separation-notice";
import {
  dismissSpeakerSeparationNotice,
  isSpeakerSeparationNoticeDismissed,
  type TranscriptChannelSummary,
  unsplitTranscriptCandidate,
} from "./speaker-separation-notice-condition";

vi.mock("~/db", () => ({ useLiveQuery: vi.fn(() => ({ data: [] })) }));

describe("unsplitTranscriptCandidate", () => {
  const row = (
    sessionChannelCount: number,
    extra: Partial<TranscriptChannelSummary> = {},
  ): TranscriptChannelSummary => ({
    provider: "openai",
    model: "gpt-transcribe",
    wordCount: 10,
    sessionChannelCount,
    sessionProviderSpeakerCount: 0,
    sessionAssignedHumanCount: 0,
    ...extra,
  });

  it("flags a transcript with all words on one channel", () => {
    expect(unsplitTranscriptCandidate([row(1)])).toEqual({
      provider: "openai",
      model: "gpt-transcribe",
    });
  });

  it("flags words without any channel (fail-closed, Forge M5)", () => {
    expect(unsplitTranscriptCandidate([row(0)])).not.toBeNull();
  });

  it("leaves separated transcripts alone, by the same rule as the labels", () => {
    expect(unsplitTranscriptCandidate([row(2)])).toBeNull();
    expect(
      unsplitTranscriptCandidate([row(1, { sessionProviderSpeakerCount: 2 })]),
    ).toBeNull();
    expect(
      unsplitTranscriptCandidate([row(1, { sessionAssignedHumanCount: 2 })]),
    ).toBeNull();
  });

  it("needs words and a provider", () => {
    expect(unsplitTranscriptCandidate([])).toBeNull();
    expect(unsplitTranscriptCandidate([row(1, { wordCount: 0 })])).toBeNull();
    expect(unsplitTranscriptCandidate([row(1, { provider: null })])).toBeNull();
  });

  it("takes provider and model of the last transcript", () => {
    expect(
      unsplitTranscriptCandidate([
        row(1, { provider: "anarlog" }),
        row(1, { provider: "openrouter", model: null }),
      ]),
    ).toEqual({ provider: "openrouter", model: null });
  });
});

describe("dismissal", () => {
  it("is remembered per session", () => {
    expect(isSpeakerSeparationNoticeDismissed("s-a")).toBe(false);
    dismissSpeakerSeparationNotice("s-a");
    expect(isSpeakerSeparationNoticeDismissed("s-a")).toBe(true);
    expect(isSpeakerSeparationNoticeDismissed("s-b")).toBe(false);
  });
});

describe("TranscriptSpeakerSeparationNotice", () => {
  afterEach(() => cleanup());

  it("asks before re-transcribing and mentions the cost", async () => {
    const onRegenerate = vi.fn(() => Promise.resolve());
    render(
      <TranscriptSpeakerSeparationNotice
        onRegenerate={onRegenerate}
        onDismiss={() => {}}
      />,
    );

    expect(
      screen.getAllByText("Your cloud provider charges for this again.").length,
    ).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("button", { name: "Re-transcribe" }));
    expect(onRegenerate).not.toHaveBeenCalled();

    const buttons = screen.getAllByRole("button", { name: "Re-transcribe" });
    const confirm = buttons[buttons.length - 1]!;
    fireEvent.click(confirm);
    expect(onRegenerate).toHaveBeenCalledTimes(1);
  });

  it("can be dismissed", () => {
    const onDismiss = vi.fn();
    render(
      <TranscriptSpeakerSeparationNotice
        onRegenerate={() => Promise.resolve()}
        onDismiss={onDismiss}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });
});
