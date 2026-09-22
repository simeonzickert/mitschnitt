import { useLingui } from "@lingui/react/macro";

/**
 * The names the retention steps go by, in one place.
 *
 * Mitschnitt-Fork (F18). Two screens now put a deadline in front of a person and
 * ask them to agree to a deletion: the picker in the settings, and the one-time
 * question the cleanup pass asks on an installation that has never armed the
 * deadline. Two copies of this map would drift, and the drift would show up as
 * the same deadline being called two different things in two dialogs about the
 * same recordings.
 *
 * Keys that are no longer offered stay here: a machine set to one of them still
 * has to see its own setting written out rather than a raw identifier.
 */
export function useAudioRetentionLabels(): Record<string, string> {
  const { t } = useLingui();
  return {
    none: t`Don't save`,
    oneDay: t`1 day`,
    threeDays: t`3 days`,
    oneWeek: t`1 week`,
    oneMonth: t`1 month`,
    thirtyDays: t`30 days`,
    threeMonths: t`3 months`,
    sixMonths: t`6 months`,
    oneYear: t`1 year`,
    forever: t`Never delete`,
  };
}
