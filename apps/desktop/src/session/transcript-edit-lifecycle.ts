import { useEffect } from "react";
import { useHotkeys } from "react-hotkeys-hook";

import { useTranscriptEditable } from "~/session/transcript-editable";

/**
 * Ends the edit mode the moment the transcript stops being editable (stop ->
 * post-stop refinement, running batch). `endEditMode` blurs the focused
 * editor first, so a half-typed correction is saved before the refinement
 * replaces the transcript (Fix-Runde B).
 */
export function useEndEditModeWhenLocked({
  sessionId,
  editMode,
  endEditMode,
}: {
  sessionId: string;
  editMode: boolean;
  endEditMode: () => void;
}) {
  const editable = useTranscriptEditable(sessionId);
  useEffect(() => {
    if (editMode && !editable) {
      endEditMode();
    }
  }, [editMode, editable, endEditMode]);
}

/**
 * Save and Escape leave the edit mode (Upstream #bf8b3abeca). `preventDefault`
 * is deliberate: the global Escape shortcut (back navigation, closing the note
 * window) skips events that are already default-prevented, so leaving the edit
 * mode must not also navigate away. Half-typed text is covered by the
 * rescue-save when the editor unmounts.
 */
export function useEndEditModeHotkeys({
  enabled,
  endEditMode,
}: {
  enabled: boolean;
  endEditMode: () => void;
}) {
  useHotkeys(
    "mod+s, escape",
    () => endEditMode(),
    {
      enabled,
      enableOnContentEditable: true,
      enableOnFormTags: true,
      preventDefault: true,
    },
    [endEditMode],
  );
}
