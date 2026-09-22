import { isTauri } from "@tauri-apps/api/core";

import {
  type DeepLink,
  commands as deeplink2Commands,
  events as deeplink2Events,
} from "@anlg/plugin-deeplink2";

import { subscribeThenDrainDeepLinks } from "~/shared/deeplink";
import { useMountEffect } from "~/shared/hooks/useMountEffect";

// Fork: the auth-callback, billing-refresh, Nango integration-callback and
// share-open deep links all pointed at the vendor cloud and are gone; the
// onboarding-demo callback went with the demo link on 2026-09-02 (it handed a
// localhost callback to the original product's website). Nothing is routed
// here today. The hook stays as the one place that drains queued deep links,
// so a future route starts from an empty queue.
export function useDeeplinkHandler() {
  useMountEffect(() => {
    if (!isTauri()) {
      return;
    }

    const handleDeepLink = (_payload: DeepLink) => {};

    const deepLinkSubscription = subscribeThenDrainDeepLinks({
      listen: (handler) =>
        deeplink2Events.deepLinkEvent.listen(({ payload }) => {
          handler(payload);
        }),
      takePendingDeepLinks: deeplink2Commands.takePendingDeepLinks,
      handle: handleDeepLink,
    });

    return () => {
      void deepLinkSubscription.then((fn) => fn()).catch(() => {});
    };
  });
}
