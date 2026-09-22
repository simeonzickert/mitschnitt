import "./styles/globals.css";
import "./styles/cursor.css";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createRouter, RouterProvider } from "@tanstack/react-router";
import { StrictMode, useEffect, useMemo } from "react";
import ReactDOM from "react-dom/client";

import "@anlg/ui/globals.css";
import {
  getCurrentWebviewWindowLabel,
  init as initWindowsPlugin,
} from "@anlg/plugin-windows";
import { Toaster } from "@anlg/ui/components/ui/toast";

import { AITaskWindowSyncBridge } from "./ai/task-window-sync";
import { createToolRegistry } from "./contexts/tool-registry/core";
import { captureOperationalError } from "./error-reporting";
import { AppI18nProvider } from "./i18n/provider";
import { AppLockGate } from "./lock/gate";
import { FloatingMeetingWindowHost } from "./meeting-float/host";
import { routeTree } from "./routeTree.gen";
import { EventListeners } from "./services/event-listeners";
import { TaskManager } from "./services/task-manager";
import { NachfrageHost } from "./session/nachfrage/host";
import {
  createTaskScheduler,
  TaskSchedulerProvider,
} from "./services/task-scheduler";
import { TrayRecordingSync } from "./services/tray-recording";
import { TrayScheduleSync } from "./services/tray-schedule";
import { UpdaterMeetingSync } from "./services/updater-meeting";
import { useRemoteSessionDeletionUndoListener } from "./session/hooks/useDeleteSession";
import { refreshLegacySettingsSnapshots } from "./settings/legacy-snapshots";
import { migratePlaintextAiProviderApiKeys } from "./settings/providers";
import { initializeApplicationSettings } from "./settings/queries";
import { initializeAppExitFlush } from "./shared/app-exit";
import { initializeAppStoreBuild, isAppStoreBuild } from "./shared/app-store";
import { useConfigValue } from "./shared/config";
import { ErrorComponent, NotFoundComponent } from "./shared/control";
import { LongLoadGate } from "./shared/long-load-gate";
import { startInteractionProfiler } from "./shared/perf/interaction-profiler";
import { bootstrapThemeFromSettings } from "./shared/theme/apply";
import { AppThemeProvider } from "./shared/theme/provider";
import type { ThemePreference } from "./shared/theme/resolve";
import { createAITaskStore } from "./store/zustand/ai-task";
import { listenerStore } from "./store/zustand/listener/instance";

const toolRegistry = createToolRegistry();
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      gcTime: 60_000,
    },
  },
});
const STARTUP_TASK_TIMEOUT_MS = 10_000;

const router = createRouter({
  routeTree,
  context: undefined,
  defaultErrorComponent: ErrorComponent,
  defaultNotFoundComponent: NotFoundComponent,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

function App() {
  const aiTaskStore = useMemo(() => createAITaskStore(), []);

  return (
    <>
      <AITaskWindowSyncBridge store={aiTaskStore} />
      <RouterProvider
        router={router}
        context={{
          listenerStore,
          aiTaskStore,
          toolRegistry,
        }}
      />
    </>
  );
}

function AppRoot() {
  return (
    <QueryClientProvider client={queryClient}>
      <LongLoadGate>
        <ReadyApp />
      </LongLoadGate>
    </QueryClientProvider>
  );
}

function ReadyApp() {
  const scheduler = useMemo(() => createTaskScheduler().start(), []);
  const theme = useConfigValue("theme") as ThemePreference;
  useRemoteSessionDeletionUndoListener(isMainWindow);

  useEffect(() => {
    runMainWindowStartupTasks();
  }, []);

  return (
    <AppThemeProvider>
      <AppI18nProvider>
        <TaskSchedulerProvider scheduler={scheduler}>
          <AppLockGate>
            <App />
            {isMainWindow ? <TaskManager /> : null}
            {isMainWindow ? <NachfrageHost /> : null}
            {isMainWindow ? <FloatingMeetingWindowHost /> : null}
            {isMainWindow ? <EventListeners /> : null}
            {isMainWindow ? <TrayScheduleSync /> : null}
            {isMainWindow ? <TrayRecordingSync /> : null}
            {isMainWindow && !isAppStoreBuild() ? <UpdaterMeetingSync /> : null}
            <Toaster position="bottom-right" theme={theme} />
          </AppLockGate>
        </TaskSchedulerProvider>
      </AppI18nProvider>
    </AppThemeProvider>
  );
}

initWindowsPlugin();

const isMainWindow = getCurrentWebviewWindowLabel() === "main";

if (isMainWindow) {
  void initializeAppExitFlush().catch((error) => {
    captureOperationalError(error, {
      operation: "app_exit_flush_initialize",
    });
  });
}

const rootElement = document.getElementById("root")!;

async function enableReactScanInDev() {
  if (!import.meta.env.DEV) {
    return;
  }

  try {
    const { scan } = await import("react-scan");
    scan({ enabled: true });
  } catch (error) {
    console.warn("Failed to start React Scan:", error);
  }

  startInteractionProfiler();
}

async function renderApp() {
  await Promise.all([
    bootstrapThemeFromSettings(),
    enableReactScanInDev(),
    initializeAppStoreBuild(),
  ]);
  const root = ReactDOM.createRoot(rootElement);
  root.render(
    <StrictMode>
      <AppRoot />
    </StrictMode>,
  );
}

if (!rootElement.innerHTML) {
  void renderApp();
}

function runMainWindowStartupTasks() {
  if (!isMainWindow) {
    return;
  }

  // Mitschnitt-Fork (F17). These two are ordered now, where they used to be
  // fired side by side. `initializeApplicationSettings` settles how long this
  // machine keeps recordings and WRITES the answer down, and the older answers
  // it must not overwrite -- the `save_recordings` switch, `saveAudioAfterMeeting`
  // -- live in the legacy documents that `refreshLegacySettingsSnapshots` copies
  // into the settings table. Started in parallel, the settings read wins (one
  // query against two plugin round-trips), the legacy answer is not there yet,
  // and "never asked" is written over a choice somebody made. Once. There is no
  // second chance: the next start finds a row and never revisits.
  //
  // Racing was harmless while nothing was written. It is F17 that makes the
  // order load-bearing, so the order is now stated rather than hoped for.
  void (async () => {
    await runStartupTask(
      "legacy_settings_refresh",
      refreshLegacySettingsSnapshots,
    );
    await runStartupTask(
      "application_settings_initialize",
      initializeApplicationSettings,
    );
  })();
  void runStartupTask(
    "ai_credentials_migrate",
    migratePlaintextAiProviderApiKeys,
  );
}

async function runStartupTask(
  operation: string,
  task: () => Promise<void>,
): Promise<void> {
  let timeoutHandle: ReturnType<typeof setTimeout> | null = null;

  await new Promise<void>((resolve, reject) => {
    timeoutHandle = setTimeout(() => {
      reject(new Error(`startup operation timed out: ${operation}`));
    }, STARTUP_TASK_TIMEOUT_MS);

    void task().then(resolve, reject);
  })
    .catch((error) => {
      captureOperationalError(error, { operation });
    })
    .finally(() => {
      if (timeoutHandle) {
        clearTimeout(timeoutHandle);
      }
    });
}
