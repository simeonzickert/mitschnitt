import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { TranscriptResumeNotice } from "./resume-notice";

describe("TranscriptResumeNotice", () => {
  afterEach(() => {
    cleanup();
  });

  it("shows the strip but does not regenerate on the strip button alone", () => {
    const onRegenerate = vi.fn().mockResolvedValue(undefined);

    render(<TranscriptResumeNotice onRegenerate={onRegenerate} />);

    expect(screen.getByTestId("transcript-resume-notice")).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Re-transcribe all" }));

    // The strip's own button only opens the confirmation -- it never calls
    // onRegenerate by itself (Fix-Runde B3d).
    expect(onRegenerate).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).not.toBeNull();
    expect(
      screen.getByText(
        /Manual speaker and text corrections will be replaced/,
      ),
    ).not.toBeNull();
  });

  it("regenerates only after confirming in the dialog", () => {
    const onRegenerate = vi.fn().mockResolvedValue(undefined);

    render(<TranscriptResumeNotice onRegenerate={onRegenerate} />);
    fireEvent.click(screen.getByRole("button", { name: "Re-transcribe all" }));

    const confirmButtons = screen.getAllByRole("button", {
      name: "Re-transcribe all",
    });
    // Two buttons now share the name: the strip's own (still in the
    // document behind the dialog) and the dialog's confirm button, which is
    // the last one rendered.
    fireEvent.click(confirmButtons[confirmButtons.length - 1]);

    expect(onRegenerate).toHaveBeenCalledTimes(1);
  });

  it("blocks a second confirm click while the first regenerate call is still pending", async () => {
    let resolveRegenerate: () => void = () => {};
    const onRegenerate = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveRegenerate = resolve;
        }),
    );

    render(<TranscriptResumeNotice onRegenerate={onRegenerate} />);
    fireEvent.click(screen.getByRole("button", { name: "Re-transcribe all" }));

    const confirmButtons = screen.getAllByRole("button", {
      name: "Re-transcribe all",
    });
    const confirmButton = confirmButtons[confirmButtons.length - 1];

    fireEvent.click(confirmButton);
    fireEvent.click(confirmButton);
    fireEvent.click(confirmButton);

    expect(onRegenerate).toHaveBeenCalledTimes(1);

    resolveRegenerate();
    await vi.waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
  });

  it("lets cancel close the dialog without regenerating", () => {
    const onRegenerate = vi.fn().mockResolvedValue(undefined);

    render(<TranscriptResumeNotice onRegenerate={onRegenerate} />);
    fireEvent.click(screen.getByRole("button", { name: "Re-transcribe all" }));

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(onRegenerate).not.toHaveBeenCalled();
  });
});
