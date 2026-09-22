import { t } from "@lingui/core/macro";

import { CliSettingsSections } from "./cli";
import { DevtoolsSection } from "./devtools";
import { WebhooksSection } from "./webhooks";

import { SettingsPageTitle } from "~/settings/page-title";

export { buildMcpConfiguration, getCliInstallNotification } from "./cli";

// Der "Guide"-Knopf zeigte auf docs.anarlog.so/agents/overview. Siehe
// settings/imports: kein Fremdverweis, keine erfundene eigene Adresse.
export function SettingsDevelopers() {
  return (
    <div className="flex flex-col gap-8">
      <div className="flex items-center justify-between gap-4">
        <SettingsPageTitle title={t`Developers`} />
      </div>
      <CliSettingsSections />
      <WebhooksSection />
      <DevtoolsSection />
    </div>
  );
}
