-- Baut die tote Cloud-Schicht aus der Datenbank aus.
--
-- Der Fork spricht mit keinem Server mehr. Was hier faellt, war ausschliesslich
-- Transport: die E2EE-Ablage fuer den Weg nach draussen, die Freigabe-Spiegel,
-- die Enterprise-Zustellquittungen, die Uebertragungsauftraege fuer Anhaenge und
-- die Arbeitsbereiche eines Kontos, das es nicht mehr gibt.
--
-- Wichtig ist die REIHENFOLGE: zuerst die Trigger, dann die Tabellen. Die
-- e2ee_dirty_*-Trigger haengen an sessions/transcripts/humans und schreiben bei
-- jedem Schreibvorgang in e2ee_dirty_rows. Faellt die Tabelle zuerst, laeuft
-- jedes spaetere INSERT in einen Fehler.
--
-- BEWUSST NICHT ANGEFASST:
--   * die workspace_id-Spalten auf sessions, transcripts, humans usw. Sie sind
--     lokal durchgehend '' und werden im Rust-Code als Ordnungsschluessel
--     mitgefuehrt. Sie zu entfernen hiesse, zwanzig STRICT-Tabellen neu zu
--     bauen -- viel Risiko, kein Gewinn.
--   * attachment_local_state (lokale Anhang-Verwaltung), webhook_endpoints
--     (ausgehende lokale Webhooks), session_proposals (der Rueckweg aus dem
--     Markdown-Spiegel), session_disclosure_attempts, voiceprint_*.

DROP TRIGGER IF EXISTS e2ee_dirty_action_items_insert;
DROP TRIGGER IF EXISTS e2ee_dirty_action_items_update;
DROP TRIGGER IF EXISTS e2ee_dirty_action_items_delete;
DROP TRIGGER IF EXISTS e2ee_dirty_humans_insert;
DROP TRIGGER IF EXISTS e2ee_dirty_humans_update;
DROP TRIGGER IF EXISTS e2ee_dirty_humans_delete;
DROP TRIGGER IF EXISTS e2ee_dirty_organizations_insert;
DROP TRIGGER IF EXISTS e2ee_dirty_organizations_update;
DROP TRIGGER IF EXISTS e2ee_dirty_organizations_delete;
DROP TRIGGER IF EXISTS e2ee_dirty_session_attachments_insert;
DROP TRIGGER IF EXISTS e2ee_dirty_session_attachments_update;
DROP TRIGGER IF EXISTS e2ee_dirty_session_attachments_delete;
DROP TRIGGER IF EXISTS e2ee_dirty_session_documents_insert;
DROP TRIGGER IF EXISTS e2ee_dirty_session_documents_update;
DROP TRIGGER IF EXISTS e2ee_dirty_session_documents_delete;
DROP TRIGGER IF EXISTS e2ee_dirty_session_participants_insert;
DROP TRIGGER IF EXISTS e2ee_dirty_session_participants_update;
DROP TRIGGER IF EXISTS e2ee_dirty_session_participants_delete;
DROP TRIGGER IF EXISTS e2ee_dirty_sessions_insert;
DROP TRIGGER IF EXISTS e2ee_dirty_sessions_update;
DROP TRIGGER IF EXISTS e2ee_dirty_sessions_delete;
DROP TRIGGER IF EXISTS e2ee_dirty_synced_preferences_insert;
DROP TRIGGER IF EXISTS e2ee_dirty_synced_preferences_update;
DROP TRIGGER IF EXISTS e2ee_dirty_synced_preferences_delete;
DROP TRIGGER IF EXISTS e2ee_dirty_transcripts_insert;
DROP TRIGGER IF EXISTS e2ee_dirty_transcripts_update;
DROP TRIGGER IF EXISTS e2ee_dirty_transcripts_delete;

DROP TRIGGER IF EXISTS e2ee_local_state_normalize_payload_insert;
DROP TRIGGER IF EXISTS e2ee_local_state_normalize_payload_update;
DROP TRIGGER IF EXISTS e2ee_local_state_prune_archive_update;
DROP TRIGGER IF EXISTS e2ee_local_state_prune_archive_delete;
DROP TRIGGER IF EXISTS e2ee_records_archive_before_update;
DROP TRIGGER IF EXISTS e2ee_records_archive_before_delete;
DROP TRIGGER IF EXISTS e2ee_records_clear_stale_payload_hash;
DROP TRIGGER IF EXISTS e2ee_replica_payload_hash_insert;
DROP TRIGGER IF EXISTS e2ee_replica_payload_hash_update;
DROP TRIGGER IF EXISTS e2ee_replica_payload_hash_delete;
DROP TRIGGER IF EXISTS e2ee_replica_reconciliation_insert;
DROP TRIGGER IF EXISTS e2ee_replica_reconciliation_update;
DROP TRIGGER IF EXISTS e2ee_replica_reconciliation_delete;
DROP TRIGGER IF EXISTS e2ee_witness_reconciliation_insert;
DROP TRIGGER IF EXISTS e2ee_witness_reconciliation_update;
DROP TRIGGER IF EXISTS e2ee_witness_reconciliation_delete;
DROP TRIGGER IF EXISTS e2ee_witness_records_normalize_payload_insert;
DROP TRIGGER IF EXISTS e2ee_witness_records_normalize_payload_update;
DROP TRIGGER IF EXISTS e2ee_witness_records_prune_archive_update;
DROP TRIGGER IF EXISTS e2ee_witness_records_prune_archive_delete;
DROP TRIGGER IF EXISTS validate_shared_session_cache_web_edit_base_insert;
DROP TRIGGER IF EXISTS validate_shared_session_cache_web_edit_base_update;

DROP VIEW IF EXISTS e2ee_local_state_resolved;
DROP VIEW IF EXISTS e2ee_witness_records_resolved;

DROP TABLE IF EXISTS e2ee_apply_guard;
DROP TABLE IF EXISTS e2ee_ciphertext_archive;
DROP TABLE IF EXISTS e2ee_dirty_rows;
DROP TABLE IF EXISTS e2ee_local_device;
DROP TABLE IF EXISTS e2ee_local_state;
DROP TABLE IF EXISTS e2ee_records;
DROP TABLE IF EXISTS e2ee_replica_payload_hashes;
DROP TABLE IF EXISTS e2ee_replica_pending;
DROP TABLE IF EXISTS e2ee_witness_pending;
DROP TABLE IF EXISTS e2ee_witness_records;
DROP TABLE IF EXISTS e2ee_witness_repair_pending;
DROP TABLE IF EXISTS e2ee_witness_state;

DROP TABLE IF EXISTS cloudsync_session_evictions;
DROP TABLE IF EXISTS cloudsync_writable_workspaces;

DROP TABLE IF EXISTS shared_session_attachment_cache;
DROP TABLE IF EXISTS shared_session_cache;
DROP TABLE IF EXISTS session_share_activation;
DROP TABLE IF EXISTS session_share_sync_state;

DROP TABLE IF EXISTS enterprise_session_completion_outbox;
DROP TABLE IF EXISTS enterprise_session_delivery_receipts;
DROP TABLE IF EXISTS enterprise_session_delivery_state;

DROP TABLE IF EXISTS attachment_transfer_jobs;
DROP TABLE IF EXISTS synced_preferences;

DROP TABLE IF EXISTS workspace_memberships;
DROP TABLE IF EXISTS workspaces;

DROP INDEX IF EXISTS idx_sessions_workspace_id_id;
