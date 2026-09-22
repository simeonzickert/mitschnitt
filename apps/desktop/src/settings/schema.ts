import { DEFAULT_AUDIO_RETENTION } from "~/services/audio-retention-policy";

export const SETTING_DEFINITIONS = {
  autostart: {
    type: "boolean",
    path: ["general", "autostart"],
    default: false as boolean,
  },
  automatic_updates: {
    type: "boolean",
    path: ["general", "automatic_updates"],
    default: true as boolean,
  },
  auto_stop_meetings: {
    type: "boolean",
    path: ["general", "auto_stop_meetings"],
    default: true as boolean,
  },
  auto_start_scheduled_meetings: {
    type: "boolean",
    path: ["general", "auto_start_scheduled_meetings"],
    default: true as boolean,
  },
  auto_join_scheduled_meetings: {
    type: "boolean",
    path: ["general", "auto_join_scheduled_meetings"],
    default: false as boolean,
  },
  floating_bar_enabled: {
    type: "boolean",
    path: ["general", "floating_bar_enabled"],
    default: true as boolean,
  },
  floating_bar_opacity: {
    type: "number",
    path: ["general", "floating_bar_opacity"],
    default: 0.78 as number,
  },
  live_caption_opacity: {
    type: "number",
    path: ["general", "live_caption_opacity"],
    default: 0.3 as number,
  },
  live_caption_width: {
    type: "number",
    path: ["general", "live_caption_width"],
    default: 440 as number,
  },
  live_caption_line_count: {
    type: "number",
    path: ["general", "live_caption_line_count"],
    default: 1 as number,
  },
  live_caption_position: {
    type: "string",
    path: ["general", "live_caption_position"],
    default: "topCenter" as string,
  },
  live_caption_minimized: {
    type: "boolean",
    path: ["general", "live_caption_minimized"],
    default: true as boolean,
  },
  show_app_in_dock: {
    type: "boolean",
    path: ["general", "show_app_in_dock"],
    default: true as boolean,
  },
  show_tray_icon: {
    type: "boolean",
    path: ["general", "show_tray_icon"],
    default: true as boolean,
  },
  theme: {
    type: "string",
    path: ["general", "theme"],
    default: "system" as string,
  },
  app_icon: {
    type: "string",
    path: ["general", "app_icon"],
    default: "default" as string,
  },
  save_recordings: {
    type: "boolean",
    path: ["general", "save_recordings"],
    default: true as boolean,
  },
  // Mitschnitt-Fork. Six months rather than forever (der Betreiber, 01.09.2026), and
  // when the deadline passes the recording goes from BOTH places -- the machine
  // room and the meeting folder. The transcript and the summary stay.
  audio_retention: {
    type: "string",
    path: ["general", "audio_retention"],
    default: DEFAULT_AUDIO_RETENTION as string,
  },
  // Mitschnitt-Fork (F18). Whether anybody has agreed that the deadline may
  // actually delete on THIS installation.
  //
  // F17 settled the deadline at startup, which is the right moment for a
  // machine whose library is the one it started with. It is the wrong moment
  // for a library that arrives afterwards -- a Time Machine restore, a copied
  // database, a new device in a cloud sync. The start found nothing to lose and
  // wrote down six months; a minute after the restore the cleanup pass takes a
  // year of recordings, from the meeting folders as well as the app.
  //
  // So the deadline is written at startup but not armed. The first cleanup pass
  // that would actually delete something asks instead, once, and the answer
  // lands here. False is the safe direction: an installation that never
  // answered keeps everything.
  audio_retention_confirmed: {
    type: "boolean",
    path: ["general", "audio_retention_confirmed"],
    default: false as boolean,
  },
  // Mitschnitt-Fork (F16c). Where the readable folder per meeting lives.
  // Empty means the default place next to the database. Only the readable side
  // moves -- the database itself never leaves this machine.
  mirror_root: {
    type: "string",
    path: ["general", "mirror_root"],
    default: "" as string,
  },
  remember_speakers: {
    type: "boolean",
    path: ["general", "remember_speakers"],
    default: true as boolean,
  },
  microphone_device: {
    type: "string",
    path: ["general", "microphone_device"],
    default: "" as string,
  },
  speaker_device: {
    type: "string",
    path: ["general", "speaker_device"],
    default: "" as string,
  },
  notification_event: {
    type: "boolean",
    path: ["notification", "event"],
    default: true as boolean,
  },
  notification_detect: {
    type: "boolean",
    path: ["notification", "detect"],
    default: true as boolean,
  },
  respect_dnd: {
    type: "boolean",
    path: ["notification", "respect_dnd"],
    default: false as boolean,
  },
  notification_bounce: {
    type: "boolean",
    path: ["notification", "bounce"],
    default: true as boolean,
  },
  // Retained so the general setting can honor existing per-event preferences.
  notification_bounce_summary: {
    type: "boolean",
    path: ["notification", "bounce_summary"],
    default: true as boolean,
  },
  notification_bounce_transcript: {
    type: "boolean",
    path: ["notification", "bounce_transcript"],
    default: true as boolean,
  },
  lock_app: {
    type: "boolean",
    path: ["general", "lock_app"],
    default: false as boolean,
  },
  consent_auto_send_chat: {
    type: "boolean",
    path: ["general", "consent_auto_send_chat"],
    default: false as boolean,
  },
  capture_meeting_chat: {
    type: "boolean",
    path: ["general", "capture_meeting_chat"],
    default: false as boolean,
  },
  cloud_sync_enabled: {
    type: "boolean",
    path: ["general", "cloud_sync_enabled"],
    default: true as boolean,
  },
  ai_language: {
    type: "string",
    path: ["language", "ai_language"],
    default: "en" as string,
  },
  spoken_languages: {
    type: "string",
    path: ["language", "spoken_languages"],
    default: "[]" as string,
  },
  personalization_dictionary_terms: {
    type: "string",
    path: ["personalization", "dictionary_terms"],
    default: "[]" as string,
  },
  /**
   * Vorschlaege, die der Betreiber verworfen hat -- als JSON-Liste von
   * `"kanonisch\u0000verhoert"`-Schluesseln.
   *
   * Ohne dieses Gedaechtnis legt jeder neue Lauf dieselben abgelehnten Paare
   * wieder vor, und ein Postfach, das Erledigtes zurueckbringt, liest
   * irgendwann niemand mehr.
   */
  personalization_dictionary_dismissed: {
    type: "string",
    path: ["personalization", "dictionary_dismissed"],
    default: "[]" as string,
  },
  custom_summary_instructions: {
    type: "string",
    path: ["personalization", "custom_summary_instructions"],
    default: "" as string,
  },
  custom_summary_instructions_token_aware: {
    type: "boolean",
    path: ["personalization", "custom_summary_instructions_token_aware"],
    default: false as boolean,
  },
  auto_summary_prompt: {
    type: "string",
    path: ["ai", "auto_summary_prompt"],
    default: "" as string,
  },
  summary_length: {
    type: "string",
    path: ["ai", "summary_length"],
    default: "detailed" as string,
  },
  ignored_platforms: {
    type: "string",
    path: ["notification", "ignored_platforms"],
    default: "[]" as string,
  },
  included_platforms: {
    type: "string",
    path: ["notification", "included_platforms"],
    default: "[]" as string,
  },
  mic_active_threshold: {
    type: "number",
    path: ["notification", "mic_active_threshold"],
    default: 15 as number,
  },
  current_llm_provider: {
    type: "string",
    path: ["ai", "current_llm_provider"],
  },
  current_llm_model: {
    type: "string",
    path: ["ai", "current_llm_model"],
  },
  current_stt_provider: {
    type: "string",
    path: ["ai", "current_stt_provider"],
  },
  current_stt_model: {
    type: "string",
    path: ["ai", "current_stt_model"],
  },
  local_stt_model_path: {
    type: "string",
    path: ["ai", "local_stt_model_path"],
    default: "" as string,
  },
  timezone: {
    type: "string",
    path: ["general", "timezone"],
  },
  week_start: {
    type: "string",
    path: ["general", "week_start"],
  },
  selected_template_id: {
    type: "string",
    path: ["general", "selected_template_id"],
  },
  default_meeting_share_access: {
    type: "string",
    path: ["general", "default_meeting_share_access"],
    default: "me" as string,
  },
  todo_linear_filter: {
    type: "string",
    path: ["todo", "linear_filter"],
    default: "" as string,
  },
  todo_github_repository: {
    type: "string",
    path: ["todo", "github_repository"],
    default: "" as string,
  },
  automation_draft_template: {
    type: "string",
    path: ["automations", "draft_template"],
    default: "" as string,
  },
  automation_workflows: {
    type: "string",
    path: ["automations", "workflows"],
    default: "[]" as string,
  },
  automation_markdown_export_enabled: {
    type: "boolean",
    path: ["automations", "markdown_export_enabled"],
    default: false as boolean,
  },
  automation_markdown_export_directory: {
    type: "string",
    path: ["automations", "markdown_export_directory"],
    default: "" as string,
  },
  automation_markdown_export_last_run: {
    type: "string",
    path: ["automations", "markdown_export_last_run"],
    default: "" as string,
  },
  automation_slack_recap_enabled: {
    type: "boolean",
    path: ["automations", "slack_recap_enabled"],
    default: false as boolean,
  },
  automation_slack_recap_channel: {
    type: "string",
    path: ["automations", "slack_recap_channel"],
    default: "" as string,
  },
  automation_slack_recap_last_run: {
    type: "string",
    path: ["automations", "slack_recap_last_run"],
    default: "" as string,
  },
  automation_slack_recap_processed: {
    type: "string",
    path: ["automations", "slack_recap_processed"],
    default: "" as string,
  },
  automation_linear_issues_enabled: {
    type: "boolean",
    path: ["automations", "linear_issues_enabled"],
    default: false as boolean,
  },
  automation_linear_issues_team: {
    type: "string",
    path: ["automations", "linear_issues_team"],
    default: "" as string,
  },
  automation_linear_issues_last_run: {
    type: "string",
    path: ["automations", "linear_issues_last_run"],
    default: "" as string,
  },
  automation_linear_issues_processed: {
    type: "string",
    path: ["automations", "linear_issues_processed"],
    default: "" as string,
  },
  automation_notion_update_enabled: {
    type: "boolean",
    path: ["automations", "notion_update_enabled"],
    default: false as boolean,
  },
  automation_notion_update_page: {
    type: "string",
    path: ["automations", "notion_update_page"],
    default: "" as string,
  },
  automation_notion_update_last_run: {
    type: "string",
    path: ["automations", "notion_update_last_run"],
    default: "" as string,
  },
  automation_notion_update_processed: {
    type: "string",
    path: ["automations", "notion_update_processed"],
    default: "" as string,
  },
} as const;

export type SettingKey = keyof typeof SETTING_DEFINITIONS;

type SettingTypeMap = {
  boolean: boolean;
  number: number;
  string: string;
};

export type SettingValue<K extends SettingKey> =
  SettingTypeMap[(typeof SETTING_DEFINITIONS)[K]["type"]];

export type SettingValues = {
  [K in SettingKey]?: SettingValue<K>;
};
