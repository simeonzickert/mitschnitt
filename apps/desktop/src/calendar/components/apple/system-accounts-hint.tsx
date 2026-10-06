import { t } from "@lingui/core/macro";
import { Trans } from "@lingui/react/macro";

import { commands as openerCommands } from "@anlg/plugin-opener2";
import { sonnerToast } from "@anlg/ui/components/ui/toast";
import { cn } from "@anlg/utils";

// Deep link to the "Internet Accounts" pane of macOS System Settings.
export const INTERNET_ACCOUNTS_URL =
  "x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension";

function showOpenFailed() {
  sonnerToast.error(t`Couldn’t open System Settings`, {
    description: t`Open System Settings → Internet Accounts yourself.`,
  });
}

export async function openInternetAccounts() {
  try {
    const result = await openerCommands.openUrl(INTERNET_ACCOUNTS_URL, null);
    if (result.status === "error") {
      showOpenFailed();
    }
  } catch {
    showOpenFailed();
  }
}

// Calendars are read through macOS (EventKit), so Google and Outlook
// calendars show up as soon as the account exists in macOS. People with a
// Gmail calendar otherwise assume it is not supported.
export function SystemAccountsHint({ className }: { className?: string }) {
  return (
    <p
      data-slot="system-accounts-hint"
      className={cn(["text-muted-foreground text-xs leading-5", className])}
    >
      <Trans>
        Google or Outlook calendar? Add the account in System Settings under
        Internet Accounts and turn on "Calendars". It will then show up here.
      </Trans>{" "}
      <button
        type="button"
        onClick={() => {
          void openInternetAccounts();
        }}
        className="hover:text-foreground underline transition-colors"
      >
        <Trans>Open Internet Accounts</Trans>
      </button>
    </p>
  );
}
