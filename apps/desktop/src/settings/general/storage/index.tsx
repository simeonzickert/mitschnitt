import { Trans } from "@lingui/react/macro";

import {
  LegacyMigrationCleanupRow,
  useLegacyMigrationCleanup,
} from "./legacy-cleanup";
import { MirrorFolderRow } from "./mirror-folder";
import { StorageLocationRow } from "./storage-location";

export function StorageSettingsView() {
  const { visible: legacyCleanupVisible } = useLegacyMigrationCleanup();

  return (
    <div>
      <h2 className="mb-4 font-sans text-lg font-semibold">
        <Trans>Storage</Trans>
      </h2>
      <div className="flex flex-col gap-3">
        {/* Mitschnitt-Fork (F16c): always shown -- where the meetings live is
            not a migration leftover, it is the setting people look for. */}
        <MirrorFolderRow />
        {/* Upstream #7340: where the app's own database lives, separate
            from the readable meeting mirror above. */}
        <StorageLocationRow />
        {legacyCleanupVisible && <LegacyMigrationCleanupRow />}
      </div>
    </div>
  );
}
