import { t as translate } from "@lingui/core/macro";
import {
  Fragment,
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import {
  Popover,
  PopoverAnchor,
  PopoverContent,
} from "@anlg/ui/components/ui/popover";
import { sonnerToast } from "@anlg/ui/components/ui/toast";
import { cn } from "@anlg/utils";

import { SegmentHeader } from "./segment-header";
import {
  getTranscriptSectionKey,
  getTranscriptSegmentDomId,
} from "./selection";
import { useTranscriptSelectionState } from "./selection-context";
import { SpeakerParticipantPicker } from "./speaker-assign";
import {
  getActiveLineIndex,
  groupWordsIntoLines,
  type HighlightSegment,
} from "./utils";
import { WordSpan } from "./word-span";

import { guardUserTranscriptMutation } from "~/session/transcript-editable";
import { createHighlightSegments } from "~/session/components/note-input/search/matching";
import type { Segment, SegmentWord } from "~/stt/live-segment";
import {
  splitTranscriptSpeaker,
  updateTranscriptSegmentText,
} from "~/stt/queries";

export type TranscriptSearchRenderState = {
  query: string;
  activeMatchId: string | null;
  caseSensitive: boolean;
  wholeWord: boolean;
};

export const EMPTY_TRANSCRIPT_SEARCH: TranscriptSearchRenderState = {
  query: "",
  activeMatchId: null,
  caseSensitive: false,
  wholeWord: false,
};

function getSegmentTimeRange(
  segment: Segment,
  offsetMs: number,
): { start: number; end: number } | null {
  const words = segment.words;
  if (words.length === 0) return null;
  return {
    start: offsetMs + (words[0].start_ms ?? 0),
    end: offsetMs + (words[words.length - 1].end_ms ?? 0),
  };
}

export const SegmentRenderer = memo(
  ({
    segment,
    offsetMs,
    transcriptId,
    sessionId,
    speakerLabel,
    currentMs,
    seekAndPlay,
    audioExists,
    search,
    editMode = false,
  }: {
    segment: Segment;
    offsetMs: number;
    transcriptId: string;
    sessionId?: string;
    speakerLabel: string;
    currentMs: number;
    seekAndPlay: (word: SegmentWord) => void;
    audioExists: boolean;
    search: TranscriptSearchRenderState;
    editMode?: boolean;
  }) => {
    const lines = useMemo(
      () => groupWordsIntoLines(segment.words),
      [segment.words],
    );
    const { selectMode, isSelected } = useTranscriptSelectionState();
    const segmentId = getTranscriptSegmentDomId(transcriptId, segment);
    const selected = isSelected(
      getTranscriptSectionKey(transcriptId, segmentId),
    );
    const highlightSegmentsByWord = useMemo(() => {
      if (!search.query) {
        return null;
      }

      const highlights = new Map<SegmentWord, HighlightSegment[]>();
      for (const word of segment.words) {
        const displayText = getWordDisplayText(word);
        highlights.set(
          word,
          createHighlightSegments(
            displayText,
            search.query,
            search.caseSensitive,
            search.wholeWord,
          ),
        );
      }
      return highlights;
    }, [search.caseSensitive, search.query, search.wholeWord, segment.words]);

    return (
      <section
        data-transcript-id={transcriptId}
        data-transcript-segment-id={segmentId}
        data-session-id={sessionId}
        data-segment-channel={segment.key.channel}
        data-segment-speaker-index={segment.key.speaker_index ?? ""}
        data-segment-speaker-human-id={segment.key.speaker_human_id ?? ""}
        data-transcript-offset-ms={offsetMs}
        data-transcript-selected={selected ? "true" : undefined}
        className={cn([
          "rounded-lg px-2 transition-colors",
          selectMode ? "cursor-pointer" : null,
          "data-[transcript-selected=true]:bg-primary/10 data-[transcript-selected=true]:ring-primary/30 data-[transcript-selected=true]:ring-1 data-[transcript-selected=true]:ring-inset",
        ])}
      >
        <SegmentHeader
          segment={segment}
          transcriptId={transcriptId}
          sessionId={sessionId}
          label={speakerLabel}
          selected={selected}
        />

        {editMode ? (
          <EditableSegmentText
            segment={segment}
            transcriptId={transcriptId}
            sessionId={sessionId}
          />
        ) : (
          <div
            data-transcript-segment-content
            className={cn([
              "overflow-wrap-anywhere mt-1.5 text-sm leading-relaxed wrap-break-word",
              selectMode ? "select-none" : "select-text-deep",
            ])}
          >
            {lines.map((line, lineIdx) => {
              const lineStartMs = offsetMs + line.startMs;
              const lineEndMs = offsetMs + line.endMs;
              const isCurrentLine =
                audioExists &&
                currentMs > 0 &&
                currentMs >= lineStartMs &&
                currentMs <= lineEndMs;

              return (
                <span
                  key={line.words[0]?.id ?? `line-${lineIdx}`}
                  data-line-current={isCurrentLine ? "true" : undefined}
                  className={cn([
                    "-mx-0.5 rounded-xs px-0.5",
                    isCurrentLine && "bg-yellow-100/50 dark:bg-yellow-900/30",
                  ])}
                >
                  {lineIdx > 0 ? " " : null}
                  {line.words.map((word, idx) => (
                    <Fragment key={word.id ?? `${word.start_ms}-${idx}`}>
                      {idx > 0 ? " " : null}
                      <WordSpan
                        word={word}
                        displayText={getWordDisplayText(word)}
                        audioExists={audioExists}
                        onClickWord={seekAndPlay}
                        highlightSegments={
                          highlightSegmentsByWord?.get(word) ?? undefined
                        }
                        isActiveMatch={
                          Boolean(word.id) && word.id === search.activeMatchId
                        }
                      />
                    </Fragment>
                  ))}
                </span>
              );
            })}
          </div>
        )}
      </section>
    );
  },
  (prev, next) => {
    if (
      prev.segment !== next.segment ||
      prev.offsetMs !== next.offsetMs ||
      prev.transcriptId !== next.transcriptId ||
      prev.sessionId !== next.sessionId ||
      prev.speakerLabel !== next.speakerLabel ||
      prev.audioExists !== next.audioExists ||
      prev.seekAndPlay !== next.seekAndPlay ||
      prev.editMode !== next.editMode
    ) {
      return false;
    }

    if (!canReuseSegmentForSearch(prev, next)) {
      return false;
    }

    if (prev.currentMs === next.currentMs) return true;

    const range = getSegmentTimeRange(prev.segment, prev.offsetMs);
    if (!range) return true;

    const prevInRange =
      prev.currentMs > 0 &&
      prev.currentMs >= range.start &&
      prev.currentMs <= range.end;
    const nextInRange =
      next.currentMs > 0 &&
      next.currentMs >= range.start &&
      next.currentMs <= range.end;

    if (!prevInRange && !nextInRange) return true;

    return (
      getActiveLineIndex(prev.segment.words, prev.offsetMs, prev.currentMs) ===
      getActiveLineIndex(next.segment.words, next.offsetMs, next.currentMs)
    );
  },
);

const EditableSegmentText = memo(function EditableSegmentText({
  segment,
  transcriptId,
  sessionId,
}: {
  segment: Segment;
  transcriptId: string;
  sessionId?: string;
}) {
  const editorRef = useRef<HTMLDivElement>(null);
  const [speakerChange, setSpeakerChange] = useState<{
    text: string;
    offset: number;
  } | null>(null);
  const originalText = normalizeEditableTranscriptText(
    segment.words.map(getWordDisplayText).join(" "),
  );
  const wordIds = useMemo(
    () =>
      segment.words.flatMap((word) =>
        typeof word.id === "string" && word.id ? [word.id] : [],
      ),
    [segment.words],
  );
  // The split preview hides the editor, so its blur-save is skipped. The typed
  // text then lives only in the split state; every way out of the preview
  // that does not assign a speaker (Escape, click outside, edit mode ending
  // and unmounting the segment) must save it, or it is lost (Fix-Runde B2).
  const pendingSplitTextRef = useRef<string | null>(null);
  const splitInFlightRef = useRef(false);
  useEffect(() => {
    pendingSplitTextRef.current = speakerChange?.text ?? null;
  }, [speakerChange]);
  const saveTypedText = useCallback(
    (text: string) => {
      const nextText = normalizeEditableTranscriptText(text);
      if (nextText === originalText || wordIds.length === 0) {
        return;
      }
      void saveText(sessionId, transcriptId, wordIds, nextText).then(
        (outcome) => {
          // The guard refused (the transcript locked) and the editor is
          // already gone: nobody else will save this text, so rescue it.
          if (outcome === "blocked" && unmountedRef.current) {
            void saveText(undefined, transcriptId, wordIds, nextText);
          }
        },
      );
    },
    [originalText, sessionId, transcriptId, wordIds],
  );
  // Rescue-save on unmount: the edit mode can end (transcript locked for the
  // refinement, Done, tab change) in the same render that removes the editor,
  // before any blur fires. Compare the DOM text with the last saved text and
  // save a difference. Deliberately NOT behind the guard: it saves data.
  const unmountedRef = useRef(false);
  const latestRef = useRef({ originalText, wordIds, transcriptId });
  useEffect(() => {
    latestRef.current = { originalText, wordIds, transcriptId };
  }, [originalText, wordIds, transcriptId]);
  useLayoutEffect(() => {
    unmountedRef.current = false;
    return () => {
      unmountedRef.current = true;
      if (splitInFlightRef.current) return;
      const { originalText, wordIds, transcriptId } = latestRef.current;
      const editor = editorRef.current;
      const text =
        pendingSplitTextRef.current ??
        (editor ? (editor.innerText ?? editor.textContent ?? "") : null);
      if (text === null) return;
      const nextText = normalizeEditableTranscriptText(text);
      if (nextText === originalText || wordIds.length === 0) return;
      void saveText(undefined, transcriptId, wordIds, nextText);
    };
  }, []);
  const handleBlur = useCallback(
    (event: React.FocusEvent<HTMLDivElement>) => {
      if (speakerChange) return;
      saveTypedText(
        event.currentTarget.innerText ?? event.currentTarget.textContent ?? "",
      );
    },
    [saveTypedText, speakerChange],
  );
  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229)
        return;

      if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        event.currentTarget.blur();
        return;
      }
      if (event.key !== "Enter" || event.shiftKey || event.altKey) return;
      const selection = window.getSelection();
      if (!selection?.isCollapsed || !selection.rangeCount) return;
      const range = selection.getRangeAt(0);
      if (!event.currentTarget.contains(range.startContainer)) return;
      const before = range.cloneRange();
      before.selectNodeContents(event.currentTarget);
      before.setEnd(range.startContainer, range.startOffset);
      const text = event.currentTarget.textContent ?? "";
      const offset = before.toString().length;
      event.preventDefault();
      if (text.slice(offset).trim()) setSpeakerChange({ text, offset });
    },
    [],
  );

  return (
    <Popover
      open={speakerChange !== null}
      onOpenChange={(open) => {
        if (!open) {
          if (speakerChange) saveTypedText(speakerChange.text);
          setSpeakerChange(null);
        }
      }}
    >
      <div
        ref={editorRef}
        hidden={speakerChange !== null}
        data-transcript-segment-content
        data-transcript-editor
        data-transcript-edit-word-ids={JSON.stringify(wordIds)}
        data-transcript-edit-word-start-ms={JSON.stringify(
          segment.words.map((word) => word.start_ms),
        )}
        data-transcript-edit-word-texts={JSON.stringify(
          segment.words.map(getWordDisplayText),
        )}
        contentEditable
        suppressContentEditableWarning
        spellCheck
        className={cn([
          "overflow-wrap-anywhere mt-1.5 rounded-md text-sm leading-relaxed wrap-break-word outline-hidden",
          "select-text-deep",
        ])}
        onBlur={handleBlur}
        onKeyDown={handleKeyDown}
      >
        {originalText}
      </div>
      {speakerChange && (
        <div
          data-transcript-split-preview
          className="mt-1.5 text-sm leading-relaxed wrap-break-word"
        >
          {speakerChange.text.slice(0, speakerChange.offset).trim()}
          <PopoverAnchor asChild>
            <div data-transcript-speaker-split className="mt-5">
              {speakerChange.text.slice(speakerChange.offset).trim()}
            </div>
          </PopoverAnchor>
        </div>
      )}
      <PopoverContent
        variant="app"
        side="bottom"
        align="start"
        collisionPadding={12}
        className="flex max-h-(--radix-popover-content-available-height) w-80 max-w-[calc(100vw-24px)] flex-col overflow-hidden"
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          requestAnimationFrame(() =>
            editorRef.current?.focus({ preventScroll: true }),
          );
        }}
      >
        <SpeakerParticipantPicker
          sessionId={sessionId}
          showAssignmentScope={false}
          onSelect={async (humanId) => {
            if (!speakerChange) return;
            splitInFlightRef.current = true;
            const typed = speakerChange.text;
            let split = false;
            try {
              const result = await guardUserTranscriptMutation(
                sessionId,
                (assertEditable) =>
                  splitTranscriptSpeaker({
                    transcriptId,
                    segmentKey: segment.key,
                    wordIds,
                    ...speakerChange,
                    humanId,
                    assertEditable,
                  }),
              );
              if (!result.allowed) {
                return;
              }
              if (!result.value) {
                // No word matched: the transcript changed under the editor.
                throw new Error("split matched no words");
              }
              split = true;
              pendingSplitTextRef.current = null;
              setSpeakerChange(null);
            } catch (error) {
              console.error("[transcript] failed to split block", error);
              sonnerToast.error(translate`The block could not be split`);
            } finally {
              splitInFlightRef.current = false;
              // The editor may have been removed while the split ran (edit
              // mode ended): its unmount rescue skipped the text because a
              // split was in flight, so a failed split saves it now.
              if (!split && unmountedRef.current) {
                const rescued = normalizeEditableTranscriptText(typed);
                if (rescued !== originalText && wordIds.length > 0) {
                  void saveText(undefined, transcriptId, wordIds, rescued);
                }
              }
            }
          }}
        />
      </PopoverContent>
    </Popover>
  );
});

// A save that finds none of its words (the transcript was replaced under the
// editor) or throws must be visible, not a console line (Fix-Runde B6).
async function saveText(
  sessionId: string | undefined,
  transcriptId: string,
  wordIds: string[],
  text: string,
): Promise<"saved" | "blocked" | "failed"> {
  try {
    const result = await guardUserTranscriptMutation(
      sessionId,
      (assertEditable) =>
        updateTranscriptSegmentText({
          transcriptId,
          wordIds,
          text,
          assertEditable,
        }),
    );
    if (!result.allowed) {
      return "blocked";
    }
    if (result.value === false) {
      console.error("[transcript] text save matched no words", transcriptId);
      sonnerToast.error(translate`Your text change could not be saved`);
      return "failed";
    }
    return "saved";
  } catch (error) {
    console.error("[transcript] failed to update text", error);
    sonnerToast.error(translate`Your text change could not be saved`);
    return "failed";
  }
}

export function normalizeEditableTranscriptText(text: string) {
  return text.replace(/\s+/g, " ").trim();
}

function canReuseSegmentForSearch(
  prev: { segment: Segment; search: TranscriptSearchRenderState },
  next: { segment: Segment; search: TranscriptSearchRenderState },
) {
  if (
    prev.search.query !== next.search.query ||
    prev.search.caseSensitive !== next.search.caseSensitive ||
    prev.search.wholeWord !== next.search.wholeWord
  ) {
    return false;
  }

  if (prev.search.activeMatchId === next.search.activeMatchId) {
    return true;
  }

  return (
    !segmentContainsWordId(prev.segment, prev.search.activeMatchId) &&
    !segmentContainsWordId(next.segment, next.search.activeMatchId)
  );
}

function segmentContainsWordId(segment: Segment, wordId: string | null) {
  if (!wordId) {
    return false;
  }

  return segment.words.some((word) => word.id === wordId);
}

function getWordDisplayText(word: SegmentWord) {
  return word.text.trim();
}
