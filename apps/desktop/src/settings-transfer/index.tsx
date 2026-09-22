import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import {
  CheckCircle,
  DownloadSimple,
  FolderOpen,
  UploadSimple,
  Warning,
} from "@phosphor-icons/react";
import {
  open as selectFile,
  save as selectTarget,
} from "@tauri-apps/plugin-dialog";
import { useCallback, useState } from "react";

import type { LocalModel } from "@anlg/plugin-local-stt";
import { Button } from "@anlg/ui/components/ui/button";
import { Input } from "@anlg/ui/components/ui/input";

import {
  applyImport,
  collectBundlePayload,
  parseBundlePayload,
  PartialImportError,
  planImport,
  type ImportPlan,
  type ImportStage,
  type SettingsBundlePayload,
} from "./bundle";
import { MIN_PASSWORD_CHARS } from "./keys";
import { checkSelectedLocalModel, type LocalModelState } from "./model";

import { SettingsPageTitle } from "~/settings/page-title";
import { SettingSwitchRow } from "~/settings/setting-row";
import { useLocalModelDownload } from "~/stt/useLocalSttModel";
import { commands as tauriCommands, type BundleInfo } from "~/types/tauri.gen";

/**
 * Mitschnitt-Fork (F14). One person sets Mitschnitt up, exports a file, the
 * others import it and are ready to work.
 *
 * The import never writes before it has shown what it would change: a bundle
 * comes from another machine, and overwriting someone's setup without telling
 * them what moved is the kind of surprise that makes people stop using a
 * feature.
 */
export function SettingsTransfer() {
  return (
    <div className="flex flex-col gap-10">
      <SettingsPageTitle title={<Trans>Export and import</Trans>} />
      <ExportSection />
      <div className="border-border border-t" />
      <ImportSection />
    </div>
  );
}

const transferMessages = {
  password_required: msg`This file is protected by a password.`,
  wrong_password: msg`The password does not fit this file. Nothing was imported.`,
  empty_password: msg`Please enter a password.`,
  password_too_short: msg`The password must be at least 6 characters long.`,
  not_a_bundle: msg`This file is not a Mitschnitt settings file.`,
  unsupported_version: msg`This file was written by a newer version of Mitschnitt.`,
  malformed: msg`This settings file is damaged.`,
  randomness_unavailable: msg`This machine could not produce the randomness the encryption needs.`,
  secret_in_plain_bundle: msg`Nothing was written: the file would have carried an access key unprotected.`,
  io: msg`The file could not be read or written.`,
  unreasonable_kdf_cost: msg`This file demands an unreasonable amount of work to open. Nothing was imported.`,
  inconsistent_bundle: msg`This file contradicts itself about its access credentials. Nothing was imported.`,
  unknown: msg`Something went wrong.`,
  passwords_differ: msg`The two passwords are not the same.`,
  import_failed: msg`The import could not be completed.`,
};

const stageNames: Record<ImportStage, MessageDescriptor> = {
  templates: msg`summary templates`,
  providers: msg`providers`,
  settings: msg`settings`,
};

/**
 * The import writes into three places that share no transaction, so a failure
 * partway leaves some of it applied. The message says which -- a bare
 * "something went wrong" would leave the person unable to tell whether their
 * setup is untouched or half replaced.
 *
 * `templates` and `providers` write one item at a time, so the STAGE a
 * failure happened in can itself be partly written: the first item landed,
 * the second one is what threw. `cause.appliedItems` names those -- without
 * it, a failure on the second template read as "nothing was applied yet"
 * while a template already sat in the database.
 */
export function importFailureMessage(
  i18n: { _: (descriptor: MessageDescriptor) => string },
  cause: unknown,
): string {
  const headline = i18n._(transferMessages.import_failed);
  if (!(cause instanceof PartialImportError)) return headline;

  const list = (stages: ImportStage[]) =>
    stages.map((stage) => i18n._(stageNames[stage])).join(", ");

  const parts = [headline];

  if (cause.applied.length > 0) {
    parts.push(i18n._(msg`Already applied: ${list(cause.applied)}.`));
  } else if (cause.appliedItems.length === 0) {
    parts.push(i18n._(msg`Nothing was applied yet.`));
  }

  if (cause.appliedItems.length > 0) {
    const stageLabel = i18n._(stageNames[cause.stage]);
    const items = cause.appliedItems.join(", ");
    parts.push(i18n._(msg`Already wrote part of ${stageLabel}: ${items}.`));
  }

  parts.push(i18n._(msg`Not applied: ${list(cause.pending)}.`));
  parts.push(
    i18n._(msg`Running the import again is safe and completes the rest.`),
  );

  return parts.join(" ");
}

/**
 * Rust hands over a stable code, never a sentence, so the wording a person
 * reads stays in the translation catalogs.
 */
export function transferErrorMessage(code: string): MessageDescriptor {
  if (code in transferMessages) {
    return transferMessages[code as keyof typeof transferMessages];
  }
  return code.startsWith("io:")
    ? transferMessages.io
    : transferMessages.unknown;
}

function ExportSection() {
  const { i18n, t } = useLingui();
  const [includeSecrets, setIncludeSecrets] = useState(false);
  const [password, setPassword] = useState("");
  const [repeated, setRepeated] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedTo, setSavedTo] = useState<string | null>(null);

  const run = useCallback(async () => {
    setError(null);
    setSavedTo(null);

    if (includeSecrets) {
      if (!password) {
        setError(i18n._(transferMessages.empty_password));
        return;
      }
      // Betreiber, 01.09.2026: at least six characters. The core enforces the
      // same rule, so this is the friendly half, not the real one -- and it
      // counts characters, matching the core, so six umlauts pass here too.
      if ([...password].length < MIN_PASSWORD_CHARS) {
        setError(i18n._(transferMessages.password_too_short));
        return;
      }
      if (password !== repeated) {
        setError(i18n._(transferMessages.passwords_differ));
        return;
      }
    }

    const target = await selectTarget({
      title: t`Save settings file`,
      defaultPath: "mitschnitt-einstellungen.json",
      filters: [{ name: "Mitschnitt", extensions: ["json"] }],
    });
    if (!target) return;

    setBusy(true);
    try {
      const payload = await collectBundlePayload({ includeSecrets });
      const result = await tauriCommands.exportSettingsBundle(
        target,
        JSON.stringify(payload),
        includeSecrets,
        includeSecrets ? password : null,
      );
      if (result.status === "error") {
        setError(i18n._(transferErrorMessage(result.error)));
        return;
      }
      setSavedTo(target);
      setPassword("");
      setRepeated("");
    } finally {
      setBusy(false);
    }
  }, [i18n, includeSecrets, password, repeated, t]);

  return (
    <section className="flex flex-col gap-5">
      <div>
        <h3 className="text-sm font-medium">
          <Trans>Export</Trans>
        </h3>
        <p className="text-muted-foreground mt-1 text-xs leading-5">
          <Trans>
            Writes your setup to a file: model choice, language, summary
            templates, word list, and the notification and channel-watch
            settings. Meetings, recordings and everything else in the database
            stay on this machine.
          </Trans>
        </p>
      </div>

      <SettingSwitchRow
        title={<Trans>Include access credentials</Trans>}
        description={
          <Trans>
            Takes the API keys of your cloud providers along. The file is then
            encrypted and only opens with the password.
          </Trans>
        }
        checked={includeSecrets}
        onChange={(checked) => {
          setIncludeSecrets(checked);
          setError(null);
        }}
      />

      {includeSecrets ? (
        <div className="flex flex-col gap-2">
          <Input
            type="password"
            autoComplete="new-password"
            value={password}
            placeholder={t`Password`}
            aria-label={t`Password`}
            onChange={(event) => setPassword(event.target.value)}
          />
          <Input
            type="password"
            autoComplete="new-password"
            value={repeated}
            placeholder={t`Repeat password`}
            aria-label={t`Repeat password`}
            onChange={(event) => setRepeated(event.target.value)}
          />
        </div>
      ) : null}

      <div className="flex items-center gap-3">
        <Button type="button" onClick={() => void run()} disabled={busy}>
          <UploadSimple className="size-4" />
          <Trans>Save file</Trans>
        </Button>
        {savedTo ? (
          <span className="text-muted-foreground flex items-center gap-1.5 text-xs">
            <CheckCircle className="size-3.5" />
            {savedTo}
          </span>
        ) : null}
      </div>

      {error ? <ErrorLine message={error} /> : null}
    </section>
  );
}

type Loaded = {
  path: string;
  info: BundleInfo;
  payload: SettingsBundlePayload;
  plan: ImportPlan;
};

function ImportSection() {
  const { i18n, t } = useLingui();
  const [path, setPath] = useState<string | null>(null);
  const [info, setInfo] = useState<BundleInfo | null>(null);
  const [password, setPassword] = useState("");
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [applied, setApplied] = useState(false);
  // Deliberately not remembered across files: every bundle has to be agreed to
  // on its own, and `reset` clears it with everything else.
  const [confirmedEndpoints, setConfirmedEndpoints] = useState(false);
  const [modelState, setModelState] = useState<LocalModelState | null>(null);

  const reset = useCallback(() => {
    setInfo(null);
    setPassword("");
    setLoaded(null);
    setError(null);
    setApplied(false);
    setConfirmedEndpoints(false);
    setModelState(null);
  }, []);

  const load = useCallback(
    async (target: string, secret: string | null) => {
      setBusy(true);
      setError(null);
      try {
        const result = await tauriCommands.readSettingsBundle(target, secret);
        if (result.status === "error") {
          setError(i18n._(transferErrorMessage(result.error)));
          setLoaded(null);
          return;
        }
        const payload = parseBundlePayload(JSON.parse(result.data.payload));
        const plan = await planImport(payload);
        setLoaded({ path: target, info: result.data.info, payload, plan });
      } finally {
        setBusy(false);
      }
    },
    [i18n],
  );

  const choose = useCallback(async () => {
    const selected = await selectFile({
      title: t`Choose settings file`,
      multiple: false,
      directory: false,
      filters: [{ name: "Mitschnitt", extensions: ["json"] }],
    });
    if (typeof selected !== "string") return;

    reset();
    setPath(selected);
    const result = await tauriCommands.inspectSettingsBundle(selected);
    if (result.status === "error") {
      setError(i18n._(transferErrorMessage(result.error)));
      return;
    }
    setInfo(result.data);
    if (!result.data.encrypted) {
      await load(selected, null);
    }
  }, [i18n, load, reset, t]);

  const apply = useCallback(async () => {
    if (!loaded) return;
    setBusy(true);
    setError(null);
    try {
      await applyImport(loaded.payload, { confirmedEndpoints });
      setApplied(true);
      // The decrypted payload has done its job. Holding it in component state
      // after that keeps every imported API key in memory for as long as the
      // settings tab stays open, for no benefit.
      setLoaded(null);
      setModelState(await checkSelectedLocalModel());
    } catch (cause) {
      setError(importFailureMessage(i18n, cause));
    } finally {
      setBusy(false);
    }
  }, [confirmedEndpoints, i18n, loaded]);

  return (
    <section className="flex flex-col gap-5">
      <div>
        <h3 className="text-sm font-medium">
          <Trans>Import</Trans>
        </h3>
        <p className="text-muted-foreground mt-1 text-xs leading-5">
          <Trans>
            Reads a settings file from a colleague. You will see what would
            change before anything is written.
          </Trans>
        </p>
      </div>

      <div className="flex items-center gap-3">
        <Button type="button" variant="outline" onClick={() => void choose()}>
          <FolderOpen className="size-4" />
          <Trans>Choose file</Trans>
        </Button>
        {path ? (
          <span className="text-muted-foreground truncate text-xs">{path}</span>
        ) : null}
      </div>

      {info?.encrypted && !loaded ? (
        <div className="flex flex-col gap-2">
          <p className="text-muted-foreground text-xs">
            <Trans>This file is protected by a password.</Trans>
          </p>
          <div className="flex items-center gap-2">
            <Input
              type="password"
              autoComplete="off"
              value={password}
              placeholder={t`Password`}
              aria-label={t`Password`}
              onChange={(event) => setPassword(event.target.value)}
            />
            <Button
              type="button"
              disabled={busy || !password || !path}
              onClick={() => void (path && load(path, password))}
            >
              <Trans>Open</Trans>
            </Button>
          </div>
        </div>
      ) : null}

      {loaded && !applied ? (
        <ImportPreview plan={loaded.plan} info={loaded.info} />
      ) : null}

      {loaded && !applied ? (
        <div className="flex flex-col gap-3">
          {/*
            A bundle that points a provider somewhere other than that
            provider's own address needs a deliberate act, not a glance. The
            honest version of the same thing -- a company proxy, LM Studio,
            Ollama on another machine -- is ordinary enough that refusing it
            outright would be wrong, so this is a confirmation rather than a
            ban. Because it is required, the default outcome of an inattentive
            import is that nothing is written and no key goes anywhere.
          */}
          {loaded.plan.providers.some((change) => !change.isDefaultEndpoint) ? (
            <label className="flex items-start gap-2 text-xs">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={confirmedEndpoints}
                onChange={(event) =>
                  setConfirmedEndpoints(event.target.checked)
                }
              />
              <span>
                <Trans>
                  I know this file sends my meetings, and the access keys on
                  this machine, to the addresses shown above.
                </Trans>
              </span>
            </label>
          ) : null}
          <div>
            <Button
              type="button"
              onClick={() => void apply()}
              disabled={
                busy ||
                (loaded.plan.providers.some(
                  (change) => !change.isDefaultEndpoint,
                ) &&
                  !confirmedEndpoints)
              }
            >
              <DownloadSimple className="size-4" />
              <Trans>Apply</Trans>
            </Button>
          </div>
        </div>
      ) : null}

      {applied ? (
        <div
          className="flex flex-col gap-3"
          // The one observable trace of whether the decrypted payload is still
          // in component state. Without it the H10 claim is untestable: hiding
          // the preview is driven by `applied`, so the UI looks identical
          // whether or not the payload was let go, and a test written against
          // the UI alone would pass on the broken version too.
          data-payload-held={loaded ? "true" : "false"}
        >
          <p className="flex items-center gap-1.5 text-sm">
            <CheckCircle className="size-4" />
            <Trans>The setup has been applied.</Trans>
          </p>
          <ModelFollowUp state={modelState} />
        </div>
      ) : null}

      {error ? <ErrorLine message={error} /> : null}
    </section>
  );
}

function ImportPreview({ plan, info }: { plan: ImportPlan; info: BundleInfo }) {
  const nothing =
    plan.settings.length === 0 &&
    plan.providers.length === 0 &&
    plan.templates.length === 0;

  return (
    <div className="border-border bg-card flex flex-col gap-3 rounded-2xl border p-5">
      <h4 className="text-sm font-medium">
        <Trans>What this import changes</Trans>
      </h4>
      <p className="text-muted-foreground text-xs">
        <Trans>
          Written on {info.created_at} with Mitschnitt {info.app_version}.
        </Trans>
      </p>

      {nothing ? (
        <p className="text-muted-foreground text-xs">
          <Trans>Nothing would change; this setup is already in place.</Trans>
        </p>
      ) : null}

      {plan.settings.length > 0 ? (
        <div>
          <p className="text-xs font-medium">
            <Trans>Settings</Trans>
          </p>
          <ul className="text-muted-foreground mt-1 flex flex-col gap-0.5 text-xs">
            {plan.settings.map((change) => (
              <li key={change.key} data-testid={`change-${change.key}`}>
                {change.key}: {formatValue(change.before)} {"->"}{" "}
                {formatValue(change.after)}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {plan.providers.length > 0 ? (
        <div>
          <p className="text-xs font-medium">
            <Trans>Providers</Trans>
          </p>
          <ul className="text-muted-foreground mt-1 flex flex-col gap-0.5 text-xs">
            {plan.providers.map((change) => (
              <li key={`${change.type}:${change.providerId}`}>
                {change.type}: {change.providerId}
                {change.carriesKey ? (
                  <>
                    {" "}
                    <Trans>(with access key)</Trans>
                  </>
                ) : (
                  <>
                    {" "}
                    <Trans>(without access key)</Trans>
                  </>
                )}
                {/*
                  The address, in full, always. It is the field in a bundle
                  that can do the most damage and it used to be the only one
                  the preview did not show: a readable, password-free file
                  could point a provider at any host, and all a person saw was
                  "without access key".
                */}
                {change.baseUrl ? (
                  <div className="font-mono break-all">{change.baseUrl}</div>
                ) : null}
                {change.isDefaultEndpoint ? null : (
                  <div className="text-destructive font-medium">
                    {change.sendsExistingKeyElsewhere ? (
                      <Trans>
                        This is not {change.providerId}'s own address. Your
                        existing access key and the contents of the meetings you
                        summarise or transcribe would be sent there.
                      </Trans>
                    ) : (
                      <Trans>
                        This is not {change.providerId}'s own address. The
                        contents of the meetings you summarise or transcribe
                        would be sent there.
                      </Trans>
                    )}
                  </div>
                )}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {plan.templates.length > 0 ? (
        <div>
          <p className="text-xs font-medium">
            <Trans>Summary templates</Trans>
          </p>
          <ul className="text-muted-foreground mt-1 flex flex-col gap-0.5 text-xs">
            {plan.templates.map((change) => (
              <li key={change.id}>
                {change.title || change.id}
                {change.action === "replace" ? (
                  <>
                    {" "}
                    <Trans>(replaces a template with the same name)</Trans>
                  </>
                ) : null}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {plan.missingSelectedTemplateId ? (
        <p
          data-testid="missing-template-hint"
          className="flex items-start gap-1.5 text-xs"
        >
          <Warning className="mt-0.5 size-3.5 shrink-0" />
          <Trans>
            The file selects a summary template that is neither in it nor on
            this machine. Summaries would fall back to the default until you
            pick one in the summary settings.
          </Trans>
        </p>
      ) : null}

      {plan.unchangedSettings > 0 ? (
        <p className="text-muted-foreground text-xs">
          <Trans>
            {plan.unchangedSettings} settings already match and stay as they
            are.
          </Trans>
        </p>
      ) : null}
    </div>
  );
}

function ModelFollowUp({ state }: { state: LocalModelState | null }) {
  if (!state) return null;

  if (state.state === "local-file") {
    return (
      <p
        data-testid="local-file-model-notice"
        className="text-muted-foreground flex items-center gap-1.5 text-xs"
      >
        <Warning className="size-3.5" />
        <Trans>
          The imported setup transcribes with a model file you chose on the
          other machine. Model files are not part of a settings file -- please
          choose it again in the transcription settings.
        </Trans>
      </p>
    );
  }

  if (state.state === "unknown") {
    return (
      <p className="text-muted-foreground flex items-center gap-1.5 text-xs">
        <Warning className="size-3.5" />
        <Trans>
          Mitschnitt could not check whether the transcription model is on this
          machine. Please look at the transcription settings.
        </Trans>
      </p>
    );
  }

  if (state.state !== "missing") return null;

  return <MissingModelNotice model={state.model} />;
}

function MissingModelNotice({ model }: { model: LocalModel }) {
  const download = useLocalModelDownload(model);

  return (
    <div
      data-testid="missing-model-notice"
      className="border-border bg-card flex items-start justify-between gap-4 rounded-2xl border p-5"
    >
      <div>
        <h4 className="flex items-center gap-1.5 text-sm font-medium">
          <Warning className="size-4" />
          <Trans>The transcription model is not on this machine yet</Trans>
        </h4>
        <p className="text-muted-foreground mt-1 text-xs leading-5">
          <Trans>
            The imported setup transcribes with {model}. Without the model file
            nothing would be transcribed.
          </Trans>
        </p>
        {download.showProgress ? (
          <p className="text-muted-foreground mt-1 text-xs">
            <Trans>Downloading: {download.progress}%</Trans>
          </p>
        ) : null}
        {download.errorMessage ? (
          <p className="mt-1 text-xs text-red-600">{download.errorMessage}</p>
        ) : null}
      </div>
      <Button
        type="button"
        size="sm"
        onClick={download.handleDownload}
        disabled={download.showProgress || download.isDownloaded}
      >
        <DownloadSimple className="size-4" />
        <Trans>Download model</Trans>
      </Button>
    </div>
  );
}

function ErrorLine({ message }: { message: string }) {
  return (
    <p role="alert" className="text-xs text-red-600">
      {message}
    </p>
  );
}

function formatValue(value: boolean | number | string | undefined): string {
  if (value === undefined) return "-";
  return typeof value === "string" ? value || '""' : String(value);
}
