import { Trans } from "@lingui/react/macro";
import { useQueryClient } from "@tanstack/react-query";
import { platform } from "@tauri-apps/plugin-os";
import { motion } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";

import { cn } from "@anlg/utils";

import { CalendarSection } from "./calendar";
import {
  getInitialStep,
  getNextStep,
  getPrevStep,
  getStepStatus,
} from "./config";
import { FinalSection, finishOnboarding } from "./final";
import { FolderLocationSection } from "./folder-location";
import { ImportSection } from "./imports";
import { PermissionsSection } from "./permissions";
import { OnboardingSection } from "./shared";

import { StandaloneWindowShell } from "~/shared/window-shell";
import { type Tab, useTabs } from "~/store/zustand/tabs";

export function TabContentOnboarding({
  tab: _tab,
}: {
  tab: Extract<Tab, { type: "onboarding" }>;
}) {
  const openCurrent = useTabs((state) => state.openCurrent);

  const handleFinish = useCallback(
    (sessionId: string) => {
      openCurrent({ type: "sessions", id: sessionId });
    },
    [openCurrent],
  );

  return <OnboardingScreen onFinish={handleFinish} />;
}

function OnboardingScreen({
  onFinish,
}: {
  onFinish: (sessionId: string) => void;
}) {
  return (
    <OnboardingScreenContent
      onFinish={onFinish}
      headerClassName="px-12 pt-4 pb-8"
      headerDragRegion
    />
  );
}

export function StandaloneOnboardingScreen({
  onFinish,
}: {
  onFinish: (sessionId: string) => void;
}) {
  return (
    <StandaloneWindowShell>
      <OnboardingScreenContent
        onFinish={onFinish}
        headerClassName="px-12 pt-4 pb-8"
        headerDragRegion
      />
    </StandaloneWindowShell>
  );
}

function OnboardingScreenContent({
  onFinish,
  headerClassName,
  headerDragRegion = false,
}: {
  onFinish: (sessionId: string) => void;
  headerClassName: string;
  headerDragRegion?: boolean;
}) {
  const queryClient = useQueryClient();
  const [currentStep, setCurrentStep] = useState(getInitialStep);
  const onboardingVideoRef = useRef<HTMLVideoElement>(null);
  const currentPlatform = platform();

  const goNext = useCallback(() => {
    const next = getNextStep(currentStep);
    if (next) setCurrentStep(next);
  }, [currentPlatform, currentStep]);

  const skipCurrentStep = useCallback(() => {
    const next = getNextStep(currentStep);
    if (next) setCurrentStep(next);
  }, [currentPlatform, currentStep]);

  const goBack = useCallback(() => {
    const prev = getPrevStep(currentStep);
    if (prev) setCurrentStep(prev);
  }, [currentStep]);

  useEffect(() => {
    if (onboardingVideoRef.current) {
      onboardingVideoRef.current.playbackRate = 0.65;
    }
  }, []);

  const handleFinish = useCallback(
    (sessionId: string) => {
      void queryClient.invalidateQueries({ queryKey: ["onboarding-needed"] });
      onFinish(sessionId);
    },
    [currentPlatform, onFinish, queryClient],
  );

  return (
    <div className="bg-card relative flex h-full min-h-0 flex-col overflow-hidden">
      <div className="pointer-events-none absolute inset-0 overflow-hidden">
        <motion.div
          className="absolute inset-0"
          initial={{ opacity: 0, y: 40 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 2, ease: [0.22, 1, 0.36, 1], delay: 0.4 }}
        >
          <video
            ref={onboardingVideoRef}
            className="absolute inset-0 h-full w-full object-cover object-bottom opacity-28"
            autoPlay
            loop
            muted
            playsInline
            preload="auto"
            aria-hidden="true"
          >
            <source src="/assets/onboarding-video.mp4" type="video/mp4" />
          </video>
          <div className="from-background/8 via-background/18 absolute inset-0 bg-linear-to-t to-transparent" />
        </motion.div>
        <div className="absolute inset-x-0 top-0 h-[80%] [mask-image:linear-gradient(to_bottom,black,black_18%,rgba(0,0,0,0.9)_36%,rgba(0,0,0,0.6)_58%,transparent)] backdrop-blur-[32px]" />
        <div className="absolute inset-x-0 top-0 h-[92%] [mask-image:linear-gradient(to_bottom,black,rgba(0,0,0,0.8)_34%,rgba(0,0,0,0.35)_62%,transparent)] backdrop-blur-[12px]" />
        <div className="from-background via-background/82 via-background/97 to-background/0 absolute inset-x-0 top-0 h-[84%] bg-linear-to-b via-18% via-42%" />
        <motion.div
          className="bg-background absolute inset-0"
          initial={{ opacity: 1 }}
          animate={{ opacity: 0 }}
          transition={{ duration: 1.0, ease: "easeOut", delay: 0.1 }}
        />
      </div>

      <div
        data-tauri-drag-region={headerDragRegion || undefined}
        className={cn([
          "relative z-10 flex shrink-0 items-center",
          headerClassName,
        ])}
      >
        <h1 className="font-hand text-foreground text-4xl leading-none font-semibold tracking-normal">
          <Trans>Welcome to Mitschnitt</Trans>
        </h1>
      </div>

      <div className="scroll-fade-y relative z-10 flex-1 overflow-y-auto">
        <div className="flex flex-col gap-4 px-12 pb-16">
          <OnboardingSection
            title={<Trans>Start with permissions</Trans>}
            completedTitle={<Trans>Permissions granted</Trans>}
            description={
              currentPlatform === "macos" ? (
                <Trans>
                  Microphone and system audio are required — without them
                  Mitschnitt cannot record. macOS will ask you for each one.
                  Accessibility is optional, but without it Mitschnitt cannot
                  read the meeting chat or post the notice that a recording
                  has started.
                </Trans>
              ) : (
                <Trans>
                  Mitschnitt needs access to your microphone and system audio to
                  record and transcribe your meetings
                </Trans>
              )
            }
            status={getStepStatus("permissions", currentStep)}
            skippable={false}
            onBack={goBack}
            onNext={goNext}
          >
            <PermissionsSection onContinue={goNext} />
          </OnboardingSection>

          <OnboardingSection
            title={<Trans>Connect calendar</Trans>}
            description={
              <Trans>
                Mitschnitt will sync your calendar to get meeting reminders
              </Trans>
            }
            completedTitle={<Trans>Calendar connected</Trans>}
            status={getStepStatus("calendar", currentStep)}
            onBack={goBack}
            onNext={goNext}
            onSkip={skipCurrentStep}
          >
            <CalendarSection onContinue={goNext} />
          </OnboardingSection>

          <OnboardingSection
            title={<Trans>Bring your meeting history</Trans>}
            description={
              <Trans>
                Import notes and transcripts from the meeting apps you already
                use.
              </Trans>
            }
            completedTitle={<Trans>Meeting history imported</Trans>}
            status={getStepStatus("imports", currentStep)}
            onBack={goBack}
            onNext={goNext}
            onSkip={skipCurrentStep}
          >
            <ImportSection onContinue={goNext} onSkip={skipCurrentStep} />
          </OnboardingSection>

          <OnboardingSection
            title={<Trans>Storage</Trans>}
            description={
              <Trans>Where your notes and recordings are stored</Trans>
            }
            completedTitle={<Trans>Storage configured</Trans>}
            status={getStepStatus("folder-location", currentStep)}
            onBack={goBack}
            onNext={goNext}
            onSkip={skipCurrentStep}
          >
            <FolderLocationSection onContinue={goNext} />
          </OnboardingSection>

          <OnboardingSection
            title={<Trans>Ready to go</Trans>}
            description={
              <Trans>
                Mitschnitt is downloading the transcription model it needs to
                turn your recordings into text. This runs once and takes a few
                minutes, depending on your connection.
              </Trans>
            }
            status={getStepStatus("final", currentStep)}
            skippable={false}
            onBack={goBack}
            onNext={() => void finishOnboarding(handleFinish)}
          >
            <FinalSection onContinue={handleFinish} />
          </OnboardingSection>
        </div>
      </div>
    </div>
  );
}
