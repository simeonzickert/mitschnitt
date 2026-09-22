import { useLingui } from "@lingui/react";
import { Trans } from "@lingui/react/macro";
import { CircleNotch } from "@phosphor-icons/react";
import { useRef, useState } from "react";

import { type ModelGateState } from "./model-gate";
import { OnboardingButton } from "./shared";
import { useOnboardingModelGate } from "./use-model-gate";
import {
  getOrCreateWelcomeSession,
  setPendingWelcomeSession,
} from "./welcome-note";

import { STT } from "~/settings/ai/stt";
import { formatModelSize } from "~/settings/ai/stt/shared";
import { createSession } from "~/session/queries";
import { flushAutomaticRelaunch } from "~/shared/relaunch";
import { commands } from "~/types/tauri.gen";

export function FinalSection({
  onContinue,
}: {
  onContinue: (sessionId: string) => void;
}) {
  const { i18n } = useLingui();
  const translate = i18n._.bind(i18n);
  const [status, setStatus] = useState<"idle" | "loading" | "error">("idle");
  const finishPromiseRef = useRef<Promise<void> | null>(null);
  const welcomeSessionRef = useRef<string | null>(null);
  const [showCloudSetup, setShowCloudSetup] = useState(false);

  const { state, canFinish, sizeBytes, retry } = useOnboardingModelGate();

  const handleContinue = async () => {
    if (finishPromiseRef.current) return;
    // Gurt und Hosentraeger: der Knopf ist bereits disabled, wenn canFinish
    // false ist. Das haelt auch einen Aufrufer auf, der den Knopf umgeht.
    if (!canFinish) return;

    setStatus("loading");
    const finishPromise = finishOnboarding(onContinue, welcomeSessionRef);
    finishPromiseRef.current = finishPromise;
    try {
      await finishPromise;
    } catch (error) {
      console.error("Failed to finish onboarding", error);
      setStatus("error");
    } finally {
      finishPromiseRef.current = null;
    }
  };

  // Der Umschalt-Link bleibt sichtbar, solange der Knopf noch gesperrt ist
  // ODER ein Cloud-Anbieter bereits eingerichtet ist -- im zweiten Fall soll
  // der Nutzer die Einrichtung wieder einklappen oder aendern koennen.
  const showCloudToggle =
    !canFinish || (state.kind === "ready" && state.reason === "cloud");

  return (
    <div className="flex flex-col items-start gap-3">
      <ModelGateStatus state={state} sizeBytes={sizeBytes} onRetry={retry} />

      {showCloudToggle && (
        <div className="flex w-full flex-col items-start gap-2">
          <button
            type="button"
            onClick={() => setShowCloudSetup((value) => !value)}
            className="text-muted-foreground hover:text-foreground text-xs underline underline-offset-2 transition-colors"
          >
            {showCloudSetup ? (
              <Trans>Hide cloud provider setup</Trans>
            ) : (
              <Trans>I'd rather use a cloud provider</Trans>
            )}
          </button>
          {showCloudSetup && (
            <div className="w-full">
              <STT />
            </div>
          )}
        </div>
      )}

      <p className="text-muted-foreground max-w-sm text-xs">
        <Trans>
          Transcription runs locally on this device, no account needed.
          Summaries need a provider set up under Settings → Intelligence: an
          API key for a cloud provider such as Claude, ChatGPT, Gemini, Grok,
          or Mistral, or a locally running server like Ollama or LM Studio.
          Without a provider, you'll still get the transcript, just no
          summary.
        </Trans>
      </p>

      <OnboardingButton
        className="px-6 py-2 text-sm disabled:cursor-wait disabled:opacity-70"
        disabled={!canFinish || status === "loading"}
        onClick={() => void handleContinue()}
      >
        {status === "loading" ? (
          <span className="flex items-center gap-2">
            <CircleNotch className="size-4 animate-spin" />
            <Trans>Open Mitschnitt</Trans>
          </span>
        ) : (
          <Trans>Open Mitschnitt</Trans>
        )}
      </OnboardingButton>
      {status === "error" && (
        <p className="text-sm text-red-500" role="alert">
          {translate({
            id: "onboarding.finish-error",
            message: "Couldn't open Mitschnitt. Please try again.",
          })}
        </p>
      )}
    </div>
  );
}

function ModelGateStatus({
  state,
  sizeBytes,
  onRetry,
}: {
  state: ModelGateState;
  sizeBytes: number | null;
  onRetry: () => void;
}) {
  if (state.kind === "checking") {
    return (
      <p className="text-muted-foreground flex items-center gap-2 text-sm">
        <CircleNotch className="size-4 animate-spin" />
        <Trans>Checking for the transcription model…</Trans>
      </p>
    );
  }

  if (state.kind === "idle" || state.kind === "downloading") {
    const progress = state.kind === "downloading" ? state.progress : 0;
    const sizeLabel = formatModelSize(sizeBytes);

    return (
      <div className="flex w-full max-w-sm flex-col gap-1.5">
        <div className="text-foreground flex items-center justify-between gap-2 text-sm">
          {sizeLabel ? (
            <Trans>Downloading the transcription model ({sizeLabel})</Trans>
          ) : (
            <Trans>Downloading the transcription model</Trans>
          )}
          <span className="text-muted-foreground font-mono text-xs">
            {Math.round(progress)}%
          </span>
        </div>
        <div className="bg-muted h-1.5 w-full overflow-hidden rounded-full">
          <div
            className="bg-primary h-full rounded-full transition-[width] duration-300"
            style={{ width: `${progress}%` }}
          />
        </div>
      </div>
    );
  }

  if (state.kind === "error") {
    return (
      <div className="flex flex-col items-start gap-2">
        <p className="text-sm text-red-500" role="alert">
          {state.message}
        </p>
        {state.exhausted && (
          <p className="text-muted-foreground text-xs">
            <Trans>
              Restarting Mitschnitt will resume the download. This screen
              won't come back.
            </Trans>
          </p>
        )}
        <button
          type="button"
          onClick={onRetry}
          className="border-border/60 text-foreground hover:bg-card/75 rounded-full border px-3 py-1 text-xs font-medium transition-colors"
        >
          <Trans>Try again</Trans>
        </button>
      </div>
    );
  }

  if (state.reason === "model") {
    return (
      <p className="text-muted-foreground text-sm">
        <Trans>The transcription model is ready.</Trans>
      </p>
    );
  }

  return (
    <p className="text-muted-foreground text-sm">
      <Trans>A cloud transcription provider is set up.</Trans>
    </p>
  );
}

export async function finishOnboarding(
  onContinue?: (sessionId: string) => void,
  welcomeSessionRef?: { current: string | null },
) {
  const welcomeSessionId =
    welcomeSessionRef?.current ??
    (await getOrCreateWelcomeSession().catch((error) => {
      console.error("Failed to create welcome note", error);
      return createSession();
    }));
  if (welcomeSessionRef) {
    welcomeSessionRef.current = welcomeSessionId;
  }
  await new Promise((resolve) => setTimeout(resolve, 100));
  const result = await commands.setOnboardingNeeded(false);
  if (result.status === "error") {
    throw new Error(result.error);
  }
  await new Promise((resolve) => setTimeout(resolve, 100));
  setPendingWelcomeSession(welcomeSessionId);
  if (await flushAutomaticRelaunch()) {
    return;
  }
  setPendingWelcomeSession(null);
  onContinue?.(welcomeSessionId);
}
