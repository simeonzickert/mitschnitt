import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  retentionDeadlineAnswered,
  retentionKeepEverything,
} from "./audio-retention-answers";
import { AudioRetentionConsentDialog } from "./audio-retention-consent";

// Mitschnitt-Fork (F18). The one question an installation is asked about its
// retention deadline, and the rows each answer writes. The rows are the part
// that can go wrong quietly: a deadline armed without a span, or a span armed
// without an answer, both look fine on screen and behave wrongly a minute
// later.
describe("the retention question", () => {
  afterEach(cleanup);

  it("names the deadline and the number, and says nothing has gone yet", () => {
    render(
      <AudioRetentionConsentDialog
        policy="sixMonths"
        expiring={37}
        onConfirmDeleting={vi.fn()}
        onKeepEverything={vi.fn()}
        onDismiss={vi.fn()}
      />,
    );

    const text = screen.getByRole("dialog").textContent ?? "";
    expect(text).toContain("37");
    // The deadline written out, not the identifier it is stored under.
    expect(text).toContain("6 months");
    expect(text).not.toContain("sixMonths");
    expect(text).toContain("Nothing has been deleted");
  });

  // One click per mounted dialog, because that is the only sequence that can
  // happen: the first answer takes the dialog away. Clicking both on one mount
  // would test a screen nobody can reach.
  it.each([
    ["Keep every", "keep"],
    ["Clean up", "confirm"],
  ])("reports which answer was given: %s", (label, expected) => {
    const given: string[] = [];
    render(
      <AudioRetentionConsentDialog
        policy="sixMonths"
        expiring={2}
        onConfirmDeleting={() => given.push("confirm")}
        onKeepEverything={() => given.push("keep")}
        onDismiss={() => given.push("dismiss")}
      />,
    );

    fireEvent.click(
      screen.getByRole("button", { name: new RegExp(label, "i") }),
    );

    expect(given).toEqual([expected]);
  });

  // Found by the second-look review. `DialogContent` always renders a close X,
  // and Escape and the backdrop go the same way; without `onOpenChange` all
  // three are inert and the app is unreachable behind a modal about deleting
  // three years of recordings. Closing has to be a real, and answerless, exit.
  it("lets the question be put off without answering it", () => {
    const onConfirmDeleting = vi.fn();
    const onKeepEverything = vi.fn();
    const onDismiss = vi.fn();
    render(
      <AudioRetentionConsentDialog
        policy="sixMonths"
        expiring={2}
        onConfirmDeleting={onConfirmDeleting}
        onKeepEverything={onKeepEverything}
        onDismiss={onDismiss}
      />,
    );

    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });

    expect(onDismiss).toHaveBeenCalledTimes(1);
    expect(onConfirmDeleting).not.toHaveBeenCalled();
    expect(onKeepEverything).not.toHaveBeenCalled();
  });

  // A deadline nobody offers any more still has to read as itself here, the
  // same way the picker keeps showing it.
  it("writes out a deadline that is no longer on the list", () => {
    render(
      <AudioRetentionConsentDialog
        policy="oneWeek"
        expiring={1}
        onConfirmDeleting={vi.fn()}
        onKeepEverything={vi.fn()}
        onDismiss={vi.fn()}
      />,
    );

    expect(screen.getByRole("dialog").textContent).toContain("1 week");
  });
});

describe("what each answer writes down", () => {
  // (c) "Keep everything" is not just an answer. Recording it without moving
  // the deadline would leave six months on screen beside a library that will
  // never be cleared, and warn the next reader that they are shortening
  // something that was never in force.
  it("keep everything moves the deadline to never and arms it there", () => {
    expect(retentionKeepEverything()).toEqual({
      audio_retention: "forever",
      save_recordings: true,
      audio_retention_confirmed: true,
    });
  });

  // (d) Picking in the settings IS the answer, so the pass never asks again.
  // Both surfaces have to write the same three fields or the deadline ends up
  // half-armed; this is why they share one function rather than two literals.
  it("a deadline picked in the settings counts as the answer", () => {
    expect(retentionDeadlineAnswered("thirtyDays")).toEqual({
      audio_retention: "thirtyDays",
      save_recordings: true,
      audio_retention_confirmed: true,
    });
  });

  // Found by the cross-vendor audit. Agreeing to the question writes the SPAN
  // the dialog showed, not just the answer. Arming "whatever is stored by now"
  // would let a dialog that said six months arm thirty days, if an import
  // moved the deadline while the question sat on screen.
  it("agreeing to the question writes the deadline the question named", () => {
    expect(retentionDeadlineAnswered("sixMonths")).toMatchObject({
      audio_retention: "sixMonths",
      audio_retention_confirmed: true,
    });
  });

  // "Don't save" is the one choice that also flips the legacy switch; leaving
  // it behind would have `resolveConfigValue` reading the pair the other way.
  it("carries the legacy switch along when the choice is not to save", () => {
    expect(retentionDeadlineAnswered("none")).toEqual({
      audio_retention: "none",
      save_recordings: false,
      audio_retention_confirmed: true,
    });
  });
});
