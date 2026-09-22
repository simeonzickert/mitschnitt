import { Trans } from "@lingui/react/macro";
import { useState, type ReactNode } from "react";

import { Button } from "@anlg/ui/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@anlg/ui/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@anlg/ui/components/ui/select";

import { useAudioRetentionLabels } from "~/services/audio-retention-labels";
import {
  normalizeAudioRetention,
  OFFERED_AUDIO_RETENTION,
  retentionChangeDeletes,
} from "~/services/audio-retention-policy";
import {
  SETTING_CONTROL_CLASS,
  SettingRow,
  SettingSwitchRow,
} from "~/settings/setting-row";

export function AudioSettingsView({
  audioRetention,
  microphoneDevice,
  speakerDevice,
  rememberSpeakers,
}: {
  audioRetention: {
    value: string;
    /**
     * Whether anybody has agreed that this deadline may delete what is already
     * here (Mitschnitt-Fork F18). False makes every finite pick count as a
     * shortening, because an unarmed deadline is deleting nothing today.
     */
    armed: boolean;
    onChange: (value: string) => void;
    countExpiring?: (policy: string) => Promise<number>;
  };
  microphoneDevice: {
    value: string;
    devices: string[];
    onChange: (value: string) => void;
  };
  speakerDevice: {
    value: string;
    devices: string[];
    onChange: (value: string) => void;
  };
  rememberSpeakers: {
    value: boolean;
    onChange: (value: boolean) => void;
  };
}) {
  return (
    <div className="flex flex-col gap-4">
      <AudioDeviceRow
        title={<Trans>Microphone</Trans>}
        description={
          <Trans>Choose the microphone that captures your voice.</Trans>
        }
        value={microphoneDevice.value}
        devices={microphoneDevice.devices}
        onChange={microphoneDevice.onChange}
      />
      <AudioDeviceRow
        title={<Trans>Speakers</Trans>}
        description={
          <Trans>
            Choose the speakers that play other participants so Mitschnitt
            records them.
          </Trans>
        }
        value={speakerDevice.value}
        devices={speakerDevice.devices}
        onChange={speakerDevice.onChange}
      />
      <AudioRetentionRow
        value={audioRetention.value}
        armed={audioRetention.armed}
        onChange={audioRetention.onChange}
        countExpiring={audioRetention.countExpiring}
      />
      <SettingSwitchRow
        title={<Trans>Remember speakers</Trans>}
        description={
          <Trans>
            Build voiceprints from meeting audio so speakers you name in a
            transcript are recognized in later meetings. Voiceprints never leave
            this device, and unnamed ones are deleted after 45 days.
          </Trans>
        }
        checked={rememberSpeakers.value}
        onChange={rememberSpeakers.onChange}
      />
    </div>
  );
}

const SYSTEM_DEFAULT_DEVICE = "__system_default_device__";

function AudioDeviceRow({
  title,
  description,
  value,
  devices,
  onChange,
}: {
  title: ReactNode;
  description: ReactNode;
  value: string;
  devices: string[];
  onChange: (value: string) => void;
}) {
  const availableDevices = [...new Set(devices)].sort((a, b) =>
    a.localeCompare(b),
  );
  const selectedDeviceUnavailable =
    Boolean(value) && !availableDevices.includes(value);
  if (selectedDeviceUnavailable) {
    availableDevices.unshift(value);
  }

  return (
    <SettingRow title={title} description={description}>
      {(labelProps) => (
        <Select
          value={value || SYSTEM_DEFAULT_DEVICE}
          onValueChange={(device) =>
            onChange(device === SYSTEM_DEFAULT_DEVICE ? "" : device)
          }
        >
          <SelectTrigger {...labelProps} className={SETTING_CONTROL_CLASS}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent className="max-h-64">
            <SelectItem value={SYSTEM_DEFAULT_DEVICE}>
              <Trans>Current default</Trans>
            </SelectItem>
            {availableDevices.map((device) => (
              <SelectItem key={device} value={device}>
                {device}
                {selectedDeviceUnavailable && device === value ? (
                  <>
                    {" "}
                    <Trans>(Unavailable — using current default)</Trans>
                  </>
                ) : null}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}
    </SettingRow>
  );
}

/**
 * What the list offers, plus whatever this machine already stores.
 *
 * The shorter steps were dropped from the offer (Betreiber-Entscheid, 01.09.2026: thirty
 * days, three months, six months, a year, or never), but a machine set to one
 * of them keeps it and keeps seeing it -- a select whose current value is not
 * in its own list shows an empty box and quietly changes the setting on the
 * next touch.
 */
function retentionOptions(current: string): string[] {
  const offered: string[] = [...OFFERED_AUDIO_RETENTION];
  return offered.includes(current) ? offered : [...offered, current];
}

/**
 * The retention picker, and the question it has to ask before it shortens.
 *
 * Mitschnitt-Fork (F17). Since the deadline reaches the meeting folder as well
 * as the machine room, choosing a shorter one is a deletion, not a preference:
 * the recordings it catches are gone from everywhere at the next pass. So a
 * shorter choice is held back until the person has seen what it costs. Making it
 * *longer* asks nothing -- nothing is lost by keeping more.
 *
 * `countExpiring` is optional, and when it is missing or fails the dialog still
 * appears with a sentence instead of a number. A warning that only shows up when
 * counting works is a warning that is missing exactly when the database is
 * unhappy.
 *
 * Mitschnitt-Fork (F18), and this is the half that is easy to get wrong:
 * "shorter" is measured against what the deadline is actually DOING, not
 * against the number in the box. On an installation that has never armed its
 * deadline, the number in the box is a value nobody chose and it is deleting
 * nothing at all -- so every finite pick is a shortening, including one that
 * reads longer.
 *
 * Without that, the whole of F18 has a door straight through it: fresh laptop,
 * startup writes six months over an empty library, a restore drops three years
 * of meetings in, and the person opens the settings, sees "6 months", and picks
 * "1 year" because it feels safer. Not a shortening by the box, so no count, no
 * dialog -- and picking arms the deadline. The next pass takes everything older
 * than a year and the F18 question never appears. Found by the second-look
 * review, not by me.
 */
function AudioRetentionRow({
  value,
  armed,
  onChange,
  countExpiring,
}: {
  value: string;
  armed: boolean;
  onChange: (value: string) => void;
  countExpiring?: (policy: string) => Promise<number>;
}) {
  const [pending, setPending] = useState<{
    policy: string;
    affected: number | null;
  } | null>(null);

  const requestChange = (next: string) => {
    if (
      next === value ||
      !retentionChangeDeletes(
        normalizeAudioRetention(value),
        normalizeAudioRetention(next),
        armed,
      )
    ) {
      onChange(next);
      return;
    }

    setPending({ policy: next, affected: null });
    void countExpiring?.(next)
      .then((affected) =>
        setPending((current) =>
          current?.policy === next ? { policy: next, affected } : current,
        ),
      )
      .catch(() => undefined);
  };

  const copyByValue = useAudioRetentionLabels();

  return (
    <SettingRow
      title={<Trans>Audio file retention</Trans>}
      description={
        <Trans>
          Choose how long recordings are kept. When the time is up, the
          recording is deleted from the meeting folder as well; the transcript
          and the summary stay.
        </Trans>
      }
    >
      {(labelProps) => (
        <>
          <Select value={value} onValueChange={requestChange}>
            <SelectTrigger {...labelProps} className={SETTING_CONTROL_CLASS}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {retentionOptions(value).map((option) => (
                <SelectItem key={option} value={option}>
                  {copyByValue[option] ?? option}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Dialog
            open={pending !== null}
            onOpenChange={(open) => {
              if (!open) setPending(null);
            }}
          >
            <DialogContent>
              <DialogHeader>
                <DialogTitle>
                  <Trans>Delete recordings that are already older?</Trans>
                </DialogTitle>
                <DialogDescription>
                  {pending === null || pending.affected === null ? (
                    <Trans>
                      Every recording already older than{" "}
                      {copyByValue[pending?.policy ?? ""] ?? pending?.policy} is
                      deleted at the next cleanup, from the meeting folder as
                      well as from the app. Transcripts and summaries stay.
                    </Trans>
                  ) : (
                    <Trans>
                      {pending.affected} of your recordings are already older
                      than {copyByValue[pending.policy] ?? pending.policy}. They
                      are deleted at the next cleanup, from the meeting folder
                      as well as from the app. Transcripts and summaries stay.
                    </Trans>
                  )}
                </DialogDescription>
              </DialogHeader>
              <DialogFooter>
                <Button variant="outline" onClick={() => setPending(null)}>
                  <Trans>Keep the current setting</Trans>
                </Button>
                <Button
                  variant="destructive"
                  onClick={() => {
                    const next = pending?.policy;
                    setPending(null);
                    if (next !== undefined) onChange(next);
                  }}
                >
                  <Trans>Shorten and delete</Trans>
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        </>
      )}
    </SettingRow>
  );
}
