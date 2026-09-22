import type { SettingKey } from "~/settings/schema";

/**
 * Mitschnitt-Fork (F14). The allowlist that decides what a settings bundle may
 * carry.
 *
 * An allowlist, never "everything except a blocklist": a new setting upstream
 * adds must stay out of the file until someone looked at it, otherwise the next
 * merge quietly widens what leaves the machine.
 */
export const EXPORTED_SETTING_KEYS = [
  // Which engine transcribes and summarizes, and how.
  "current_stt_provider",
  "current_stt_model",
  "current_llm_provider",
  "current_llm_model",
  "auto_summary_prompt",
  "summary_length",

  // Language.
  "ai_language",
  "spoken_languages",

  // Wordlist and summary wording.
  "personalization_dictionary_terms",
  "personalization_dictionary_dismissed",
  "custom_summary_instructions",
  "custom_summary_instructions_token_aware",
  "selected_template_id",

  // Look and feel.
  "theme",
  "app_icon",
  "week_start",

  // How meetings are handled.
  "save_recordings",
  "audio_retention",
  "remember_speakers",
  "auto_stop_meetings",
  "auto_start_scheduled_meetings",
  "auto_join_scheduled_meetings",
  "floating_bar_enabled",
  "floating_bar_opacity",
  "live_caption_opacity",
  "live_caption_width",
  "live_caption_line_count",
  "live_caption_position",
  "live_caption_minimized",
  "show_app_in_dock",
  "show_tray_icon",

  // Notifications, including the levels the channel watch reads.
  "notification_event",
  "notification_detect",
  "respect_dnd",
  "notification_bounce",
  "notification_bounce_summary",
  "notification_bounce_transcript",
  "ignored_platforms",
  "included_platforms",
  "mic_active_threshold",
] as const satisfies readonly SettingKey[];

export type ExportedSettingKey = (typeof EXPORTED_SETTING_KEYS)[number];

const EXPORTED_SETTING_KEY_SET: ReadonlySet<string> = new Set(
  EXPORTED_SETTING_KEYS,
);

export function isExportedSettingKey(key: string): key is ExportedSettingKey {
  return EXPORTED_SETTING_KEY_SET.has(key);
}

/**
 * Deliberately outside the bundle, with the reason, so the next reader does not
 * have to guess whether something was forgotten or decided. The test in
 * `bundle.test.ts` walks every setting the app knows and fails if one is
 * neither exported nor listed here.
 */
export const WITHHELD_SETTING_KEYS: Readonly<
  Partial<Record<SettingKey, string>>
> = {
  // Consent belongs to the person sitting at the machine. Shipping it would
  // let one person answer a privacy question on behalf of everyone else.
  // (telemetry_consent / crash_reporting_consent stood here until 01.09.2026;
  // both settings left with their receivers, PostHog and Sentry, and moved to
  // LEGACY_WITHHELD_SETTING_KEYS below.)
  consent_auto_send_chat: "consent belongs to the person, not to the setup",
  capture_meeting_chat: "consent belongs to the person, not to the setup",

  // Bound to this machine.
  audio_retention_confirmed:
    "an answer about THIS machine's own library -- whether its owner agreed that the deadline may delete what is already here. Another machine cannot have given it, and a bundle that carried it would arm the deadline on a library nobody looked at",
  microphone_device: "names hardware that only exists on the source machine",
  speaker_device: "names hardware that only exists on the source machine",
  local_stt_model_path:
    "a filesystem path to a model file that only exists on the source machine",
  mirror_root:
    "a folder that only exists on the source machine; importing it would point this machine's meetings at a path it has no business writing to",
  autostart: "an operating-system registration, not a Mitschnitt setting",
  automatic_updates: "belongs to how this machine is administered",
  timezone: "personal, and the app already detects it",

  // Bound to an account or workspace the bundle does not carry.
  cloud_sync_enabled: "depends on an account the bundle does not carry",
  lock_app: "a local protection, not a shared setup",
  default_meeting_share_access:
    "depends on a workspace the bundle does not carry",
  todo_linear_filter: "holds Linear identifiers from another workspace",
  todo_github_repository: "holds a repository from another workspace",

  // Automations point at channels, teams and pages of a specific workspace and
  // additionally carry run state. Both would be wrong on another machine.
  automation_draft_template: "workspace-bound automation",
  automation_workflows: "workspace-bound automation",
  automation_markdown_export_enabled: "workspace-bound automation",
  automation_markdown_export_directory: "a path from the source machine",
  automation_markdown_export_last_run: "run state, not configuration",
  automation_slack_recap_enabled: "workspace-bound automation",
  automation_slack_recap_channel: "a Slack channel of another workspace",
  automation_slack_recap_last_run: "run state, not configuration",
  automation_slack_recap_processed: "run state, not configuration",
  automation_linear_issues_enabled: "workspace-bound automation",
  automation_linear_issues_team: "a Linear team of another workspace",
  automation_linear_issues_last_run: "run state, not configuration",
  automation_linear_issues_processed: "run state, not configuration",
  automation_notion_update_enabled: "workspace-bound automation",
  automation_notion_update_page: "a Notion page of another workspace",
  automation_notion_update_last_run: "run state, not configuration",
  automation_notion_update_processed: "run state, not configuration",
};

/**
 * Keys the app no longer knows but older bundles and older `app_settings`
 * rows still carry. They are withheld like the block above -- never exported,
 * never imported -- only they cannot live there, because `SettingKey` no
 * longer names them. Listed with the reason so a later reader does not mistake
 * their absence from the schema for a decision to let them travel.
 *
 * Mechanically they fall out of the allowlist anyway; this list makes the
 * decision explicit and gives the test in `bundle.test.ts` something to hold
 * the allowlist against.
 */
export const LEGACY_WITHHELD_SETTING_KEYS: Readonly<Record<string, string>> = {
  telemetry_consent:
    "consent for PostHog, which left the fork on 01.09.2026 -- an old answer to a question nobody asks any more",
  crash_reporting_consent:
    "consent for Sentry, which left the fork on 01.09.2026 -- same as above",
};

/**
 * The shortest password a NEW settings bundle may be written with (set by the operator,
 * 01.09.2026).
 *
 * Must stay in step with `MIN_PASSWORD_CHARS` in `crates/settings-transfer`.
 * The Rust core is the rule; this copy exists so the export form can say so
 * before the file is written rather than after. Counted in characters, like the
 * core, so six umlauts are six.
 */
export const MIN_PASSWORD_CHARS = 6;
