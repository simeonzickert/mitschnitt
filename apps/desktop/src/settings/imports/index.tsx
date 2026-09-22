import { Trans } from "@lingui/react/macro";

import { MeetingImportScreen } from "~/imports/screen";
import { SettingsPageTitle } from "~/settings/page-title";

// Der "Documentation"-Knopf zeigte auf docs.anarlog.so/imports -- die
// Dokumentation eines fremden Produkts. Fuer diesen Fork gibt es keine, und
// eine erfundene eigene Adresse waere schlimmer als keine. Deshalb ist der
// Knopf raus statt umgebogen (01.09.2026).
export function SettingsImports() {
  return (
    <div className="flex flex-col gap-8">
      <div className="flex items-center justify-between gap-4">
        <SettingsPageTitle title={<Trans>Imports</Trans>} />
      </div>
      <MeetingImportScreen />
    </div>
  );
}
