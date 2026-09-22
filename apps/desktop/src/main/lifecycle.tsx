import { useRouteContext } from "@tanstack/react-router";
import { useCallback, useEffect, useRef } from "react";

import { useLanguageModel } from "~/ai/hooks";
import { searchCalendarEvents } from "~/calendar/queries";
import { useSessionTab } from "~/chat/components/use-session-tab";
import { buildChatTools } from "~/chat/tools";
import { searchContacts } from "~/contacts/queries";
import { useRegisterTools } from "~/contexts/tool";
import { takePendingWelcomeSession } from "~/onboarding/welcome-note";
import { useSearchEngine } from "~/search/contexts/engine";
import { initEnhancerService } from "~/services/enhancer";
import { useConfigValue } from "~/shared/config";
import { useDesktopTabLifecycle } from "~/shared/desktop-tab-lifecycle";
import { useTabs } from "~/store/zustand/tabs";
import { LiveCaptureRecovery } from "~/stt/live-capture-recovery";
import { ScheduledMeetingAutoStart } from "~/stt/scheduled-auto-start";
import { MainListenerControlBridge } from "~/stt/window-control";

export function useClassicMainLifecycle() {
  const openNew = useTabs((state) => state.openNew);

  const openDefaultEmptyTab = useCallback(() => {
    openNew({ type: "empty" });
  }, [openNew]);

  const openPendingWelcomeTab = useCallback(() => {
    const welcomeSessionId = takePendingWelcomeSession();
    if (welcomeSessionId) {
      openNew({ type: "sessions", id: welcomeSessionId });
    }
  }, [openNew]);

  useDesktopTabLifecycle({
    onEmpty: openDefaultEmptyTab,
    onInitialized: openPendingWelcomeTab,
    onZeroTabs: openDefaultEmptyTab,
  });
}

export function ClassicMainServices() {
  return (
    <>
      <LiveCaptureRecovery />
      <ScheduledMeetingAutoStart />
      <MainListenerControlBridge />
      <ToolRegistration />
      <EnhancerInit />
    </>
  );
}

function ToolRegistration() {
  const { search } = useSearchEngine();

  const getContactSearchResults = searchContacts;

  const getCalendarEventSearchResults = searchCalendarEvents;

  const { getSessionId, getEnhancedNoteId } = useSessionTab();
  const openEditTab = useCallback((requestId: string) => {
    useTabs.getState().openNew({ type: "edit", requestId });
  }, []);

  useRegisterTools(
    "chat-general",
    () =>
      buildChatTools({
        search,
        getContactSearchResults,
        getCalendarEventSearchResults,
        getSessionId,
        getEnhancedNoteId,
        openEditTab,
      }),
    [
      search,
      getContactSearchResults,
      getCalendarEventSearchResults,
      getSessionId,
      getEnhancedNoteId,
      openEditTab,
    ],
  );

  return null;
}

function EnhancerInit() {
  const { aiTaskStore } = useRouteContext({
    from: "__root__",
  });

  const model = useLanguageModel("enhance");
  const selectedTemplateId = useConfigValue("selected_template_id");

  const modelRef = useRef(model);
  modelRef.current = model;
  const templateIdRef = useRef(selectedTemplateId);
  templateIdRef.current = selectedTemplateId;

  useEffect(() => {
    if (!aiTaskStore) return;

    const service = initEnhancerService({
      aiTaskStore,
      getModel: () => modelRef.current,
      getSelectedTemplateId: () => templateIdRef.current || undefined,
    });

    return () => service.dispose();
  }, [aiTaskStore]);

  return null;
}
