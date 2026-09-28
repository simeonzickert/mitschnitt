import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { useMemo } from "react";

import {
  SearchableSelect,
  type SearchableSelectOption,
} from "./searchable-select";

import { useSetSettingValue } from "~/settings/queries";
import { SettingRow } from "~/settings/setting-row";
import { useConfigValue } from "~/shared/config";

const COMMON_TIMEZONES: {
  value: string;
  label: MessageDescriptor;
  detail: string;
}[] = [
  { value: "Pacific/Honolulu", label: msg`Hawaii`, detail: "UTC-10" },
  { value: "America/Anchorage", label: msg`Alaska`, detail: "UTC-9" },
  {
    value: "America/Los_Angeles",
    label: msg`Pacific Time`,
    detail: "UTC-8",
  },
  { value: "America/Denver", label: msg`Mountain Time`, detail: "UTC-7" },
  { value: "America/Chicago", label: msg`Central Time`, detail: "UTC-6" },
  { value: "America/New_York", label: msg`Eastern Time`, detail: "UTC-5" },
  { value: "America/Sao_Paulo", label: msg`Sao Paulo`, detail: "UTC-3" },
  { value: "Atlantic/Reykjavik", label: msg`Reykjavik`, detail: "UTC+0" },
  { value: "Europe/London", label: msg`London`, detail: "UTC+0/+1" },
  { value: "Europe/Paris", label: msg`Paris`, detail: "UTC+1/+2" },
  { value: "Europe/Berlin", label: msg`Berlin`, detail: "UTC+1/+2" },
  { value: "Africa/Cairo", label: msg`Cairo`, detail: "UTC+2" },
  { value: "Europe/Moscow", label: msg`Moscow`, detail: "UTC+3" },
  { value: "Asia/Dubai", label: msg`Dubai`, detail: "UTC+4" },
  { value: "Asia/Kolkata", label: msg`India`, detail: "UTC+5:30" },
  { value: "Asia/Bangkok", label: msg`Bangkok`, detail: "UTC+7" },
  { value: "Asia/Singapore", label: msg`Singapore`, detail: "UTC+8" },
  { value: "Asia/Shanghai", label: msg`China`, detail: "UTC+8" },
  { value: "Asia/Tokyo", label: msg`Tokyo`, detail: "UTC+9" },
  { value: "Asia/Seoul", label: msg`Seoul`, detail: "UTC+9" },
  { value: "Australia/Sydney", label: msg`Sydney`, detail: "UTC+10/+11" },
  { value: "Pacific/Auckland", label: msg`Auckland`, detail: "UTC+12/+13" },
];

export function TimezoneSelector() {
  const { t, i18n } = useLingui();
  const value = useConfigValue("timezone");
  const setTimezone = useSetSettingValue("timezone");

  const systemTimezone = useMemo(() => {
    return Intl.DateTimeFormat().resolvedOptions().timeZone;
  }, []);

  const options: SearchableSelectOption[] = useMemo(
    () =>
      COMMON_TIMEZONES.map((timezone) => ({
        ...timezone,
        label: i18n._(timezone.label),
      })),
    [i18n],
  );

  const displayValue = value || systemTimezone;

  const handleChange = (val: string) => {
    setTimezone(val === systemTimezone ? "" : val);
  };

  return (
    <SettingRow
      title={<Trans>Timezone</Trans>}
      description={<Trans>Show the timeline in your preferred timezone.</Trans>}
    >
      {(labelProps) => (
        <SearchableSelect
          {...labelProps}
          value={displayValue}
          onChange={handleChange}
          options={options}
          placeholder={t`Select timezone`}
          searchPlaceholder={t`Search timezone...`}
          className="w-full"
          dropdownClassName="w-72"
        />
      )}
    </SettingRow>
  );
}
