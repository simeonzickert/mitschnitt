import { useEffect, useState } from "react";

import {
  commands as windowsCommands,
  events as windowsEvents,
  type FloatingBarState,
} from "@anlg/plugin-windows";

import { FloatingBarOverlay } from "./bar";
import { useTrackHealth } from "./use-track-health";

export function FloatingBarOverlayScreen() {
  const [state, setState] = useState<FloatingBarState | null>(null);
  const { levels, silentTracks } = useTrackHealth();

  useEffect(() => {
    document.documentElement.dataset.floatingBar = "";

    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    void windowsCommands.floatingBarCurrentState().then((result) => {
      if (cancelled || result.status === "error" || !result.data) {
        return;
      }
      setState(result.data);
    });

    windowsEvents.floatingBarOverlayState
      .listen((event) => {
        if (!cancelled) {
          setState(event.payload.state);
        }
      })
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
          return;
        }
        unlisteners.push(unlisten);
      });

    windowsEvents.floatingBarOverlayAmplitude
      .listen((event) => {
        if (cancelled) {
          return;
        }
        setState((current) =>
          current
            ? { ...current, amplitude: event.payload.amplitude }
            : current,
        );
      })
      .then((unlisten) => {
        if (cancelled) {
          unlisten();
          return;
        }
        unlisteners.push(unlisten);
      });

    return () => {
      cancelled = true;
      delete document.documentElement.dataset.floatingBar;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  if (!state) {
    return <div className="h-screen w-screen bg-transparent" />;
  }

  return (
    <div className="h-screen w-screen bg-transparent">
      <FloatingBarOverlay
        state={state}
        levels={levels}
        silentTracks={silentTracks}
        onStop={() => {
          void windowsEvents.floatingBarStop.emit({});
        }}
        onToggleExpanded={(expanded) => {
          void windowsEvents.floatingBarSettingsChange.emit({
            floatingBarOpacity: null,
            liveCaptionOpacity: null,
            liveCaptionWidth: null,
            liveCaptionLineCount: null,
            liveCaptionPosition: null,
            liveCaptionMinimized: !expanded,
          });
        }}
      />
    </div>
  );
}
