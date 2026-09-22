use super::*;

#[tokio::test]
async fn calendar_roundtrip() {
    let db = test_db().await;

    upsert_calendar(
        db.pool(),
        UpsertCalendar {
            id: "cal1",
            tracking_id_calendar: "tracking-cal-1",
            name: "Work",
            enabled: true,
            provider: "google",
            source: "team",
            color: "#123456",
            connection_id: "conn-1",
        },
    )
    .await
    .unwrap();

    let row = get_calendar(db.pool(), "cal1").await.unwrap().unwrap();
    assert_eq!(row.name, "Work");
    assert!(row.enabled);

    let rows = list_calendars(db.pool()).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "cal1");
}

#[tokio::test]
async fn event_roundtrip() {
    let db = test_db().await;

    upsert_event(
        db.pool(),
        UpsertEvent {
            id: "evt1",
            tracking_id_event: "tracking-evt-1",
            calendar_id: "cal1",
            title: "Standup",
            started_at: "2026-04-15T09:00:00Z",
            ended_at: "2026-04-15T09:30:00Z",
            location: "",
            meeting_link: "https://meet.example/1",
            description: "Daily sync",
            note: "",
            recurrence_series_id: "series-1",
            has_recurrence_rules: true,
            is_all_day: false,
            provider: "google",
            participants_json: Some("[{\"email\":\"a@example.com\"}]"),
        },
    )
    .await
    .unwrap();

    let row = get_event(db.pool(), "evt1").await.unwrap().unwrap();
    assert_eq!(row.title, "Standup");
    assert_eq!(row.calendar_id, "cal1");

    let rows = list_events(db.pool()).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "evt1");
}

#[tokio::test]
async fn template_roundtrip() {
    let db = test_db().await;

    upsert_template(
        db.pool(),
        UpsertTemplate {
            id: "template-1",
            title: "Standup",
            description: "Daily sync",
            pinned: true,
            pin_order: Some(2),
            category: Some("meetings"),
            targets_json: Some("[\"engineering\"]"),
            sections_json: "[{\"title\":\"Notes\",\"description\":\"...\"}]",
        },
    )
    .await
    .unwrap();

    let row = get_template(db.pool(), "template-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.title, "Standup");
    assert_eq!(row.targets_json.as_deref(), Some("[\"engineering\"]"));
    assert_eq!(
        row.sections_json,
        "[{\"title\":\"Notes\",\"description\":\"...\"}]"
    );
}

#[tokio::test]
async fn migrations_seed_default_templates_without_overwriting_existing_rows() {
    let db = Db::connect_memory_plain().await.unwrap();
    anlg_db_migrate::migrate(
        &db,
        anlg_db_migrate::DbSchema {
            steps: &APP_MIGRATION_STEPS[..1],
        },
    )
    .await
    .unwrap();

    upsert_template(
        db.pool(),
        UpsertTemplate {
            id: "default-daily-standup",
            title: "Custom Standup",
            description: "Keep user edit",
            pinned: true,
            pin_order: Some(1),
            category: Some("Custom"),
            targets_json: Some("[\"Team\"]"),
            sections_json: "[{\"title\":\"Custom\",\"description\":\"Keep\"}]",
        },
    )
    .await
    .unwrap();

    anlg_db_migrate::migrate(&db, schema()).await.unwrap();

    let rows = list_templates(db.pool()).await.unwrap();
    // Sieben Vorlagen nach dem Aufraeumen vom 04.09.2026 (Step
    // 20260904150000) -- plus 'default-daily-standup' als ACHTE, weil dieser
    // Test sie vorher bearbeitet hat. Genau das ist die Zusage des
    // Aufraeum-Steps: er nimmt nur mit, was Feld fuer Feld noch der
    // Auslieferungsstand ist. 'Custom Standup' ist es nicht und bleibt.
    assert_eq!(rows.len(), 8);
    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec![
            "default-client-kickoff",
            "default-daily-standup",
            "default-lecture-notes",
            "default-one-on-one-meeting",
            "default-sprint-planning",
            "default-sprint-retrospective",
            "mitschnitt-kompakt",
            "mitschnitt-standard",
        ]
    );

    let custom_row = get_template(db.pool(), "default-daily-standup")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(custom_row.title, "Custom Standup");
    assert_eq!(custom_row.description, "Keep user edit");

    // Eine BEHALTENE Vorlage kommt unberuehrt vom Nutzer durch die Kette der
    // Steps. Vorher stand hier 'default-sales-discovery-call' -- die gehoert
    // seit dem 04.09.2026 zu den dreizehn Entfernten und kann diese Zusage
    // nicht mehr tragen. Der Titel ist seit demselben Tag deutsch (Step
    // 20260904160000); 'targets_json' bleibt bewusst unangetastet und belegt
    // hier, dass der Step wirklich nur Titel, Beschreibung und Abschnitte
    // anfasst.
    let seeded_row = get_template(db.pool(), "default-client-kickoff")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(seeded_row.title, "Kunden-Kickoff");
    assert_eq!(
        seeded_row.targets_json.as_deref(),
        Some("[\"Customer Success Manager\",\"Account Manager\",\"Implementation Lead\"]")
    );
}

#[tokio::test]
async fn list_templates_returns_all_ordered_by_id() {
    let db = test_db_without_default_templates().await;

    upsert_template(
        db.pool(),
        UpsertTemplate {
            id: "template-2",
            title: "Two",
            description: "",
            pinned: false,
            pin_order: None,
            category: None,
            targets_json: None,
            sections_json: "[]",
        },
    )
    .await
    .unwrap();

    upsert_template(
        db.pool(),
        UpsertTemplate {
            id: "template-1",
            title: "One",
            description: "",
            pinned: false,
            pin_order: None,
            category: None,
            targets_json: None,
            sections_json: "[]",
        },
    )
    .await
    .unwrap();

    let rows = list_templates(db.pool()).await.unwrap();
    let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();

    assert_eq!(ids, vec!["template-1", "template-2"]);
}

#[tokio::test]
async fn template_upsert_replaces_existing_row_by_id() {
    let db = test_db_without_default_templates().await;

    upsert_template(
        db.pool(),
        UpsertTemplate {
            id: "template-1",
            title: "First",
            description: "A",
            pinned: false,
            pin_order: None,
            category: None,
            targets_json: None,
            sections_json: "[]",
        },
    )
    .await
    .unwrap();

    upsert_template(
        db.pool(),
        UpsertTemplate {
            id: "template-1",
            title: "Second",
            description: "B",
            pinned: true,
            pin_order: Some(5),
            category: Some("sales"),
            targets_json: Some("[\"exec\"]"),
            sections_json: "[{\"title\":\"Summary\",\"description\":\"Updated\"}]",
        },
    )
    .await
    .unwrap();

    let row = get_template(db.pool(), "template-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.title, "Second");
    assert_eq!(row.description, "B");
    assert!(row.pinned);
    assert_eq!(row.pin_order, Some(5));
    assert_eq!(row.category.as_deref(), Some("sales"));
    assert_eq!(row.targets_json.as_deref(), Some("[\"exec\"]"));
    assert_eq!(
        row.sections_json,
        "[{\"title\":\"Summary\",\"description\":\"Updated\"}]"
    );
    assert_eq!(list_templates(db.pool()).await.unwrap().len(), 1);
}

// C8 (8), Review 02.09.2026: die Loesch-Invariante gilt auch nativ. Zeigte
// selected_template_id auf die geloeschte Vorlage, faellt der Standard auf
// "Auto" ('""', dasselbe, was das Abwaehlen im Formular schreibt) -- sonst
// laedt die Zusammenfassung eine ID ins Leere und faellt still auf Auto.
// Bedingt geschrieben (WHERE value_json = json_quote(?)), damit ein
// gleichzeitiger Wechsel auf eine andere Vorlage nicht ueberschrieben wird.
// D4 (Fix-Runde 1d): DELETE und UPDATE sind eine Transaktion. Scheitert der
// zweite Schritt, steht die Vorlage noch -- keine Wahl, die ins Leere zeigt.
// Ein Trigger, der jedes UPDATE auf app_settings abbricht, spielt den Fehler.
#[tokio::test]
async fn template_delete_rolls_back_the_delete_when_the_reset_fails() {
    let db = test_db().await;
    upsert_template(
        db.pool(),
        UpsertTemplate {
            id: "template-1",
            title: "template-1",
            description: "",
            pinned: false,
            pin_order: None,
            category: None,
            targets_json: None,
            sections_json: "[]",
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO app_settings (id, value_json, updated_at)
         VALUES ('selected_template_id', '\"template-1\"', '2026-08-31T07:11:39.375Z')
         ON CONFLICT(id) DO UPDATE SET value_json = excluded.value_json,
                                      updated_at = excluded.updated_at",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER app_settings_refuses_updates
         BEFORE UPDATE ON app_settings
         BEGIN SELECT RAISE(ABORT, 'simulated failure after the delete'); END",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let result = delete_template(db.pool(), "template-1").await;

    assert!(result.is_err(), "der zweite Schritt sollte scheitern");
    let still_there: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM templates WHERE id = 'template-1')")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(still_there, "das DELETE ist nicht zurueckgerollt");
    let value: String =
        sqlx::query_scalar("SELECT value_json FROM app_settings WHERE id = 'selected_template_id'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(value, "\"template-1\"");
}

#[tokio::test]
async fn template_delete_resets_the_selected_template_to_auto() {
    let db = test_db().await;
    for id in ["template-1", "template-2"] {
        upsert_template(
            db.pool(),
            UpsertTemplate {
                id,
                title: id,
                description: "",
                pinned: false,
                pin_order: None,
                category: None,
                targets_json: None,
                sections_json: "[]",
            },
        )
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO app_settings (id, value_json, updated_at)
         VALUES ('selected_template_id', '\"template-1\"', '2026-08-31T07:11:39.375Z')
         ON CONFLICT(id) DO UPDATE SET value_json = excluded.value_json,
                                      updated_at = excluded.updated_at",
    )
    .execute(db.pool())
    .await
    .unwrap();

    // Eine andere Vorlage: die Wahl bleibt, samt Zeitstempel.
    delete_template(db.pool(), "template-2").await.unwrap();
    let (value, updated_at): (String, String) = sqlx::query_as(
        "SELECT value_json, updated_at FROM app_settings WHERE id = 'selected_template_id'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(value, "\"template-1\"");
    assert_eq!(updated_at, "2026-08-31T07:11:39.375Z");

    // Die gewaehlte Vorlage: zurueck auf Auto.
    delete_template(db.pool(), "template-1").await.unwrap();
    let (value, updated_at): (String, String) = sqlx::query_as(
        "SELECT value_json, updated_at FROM app_settings WHERE id = 'selected_template_id'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(value, "\"\"", "die Einstellung behauptet weiter eine Wahl");
    assert_ne!(updated_at, "2026-08-31T07:11:39.375Z");
}

#[tokio::test]
async fn template_delete_removes_row() {
    let db = test_db().await;

    upsert_template(
        db.pool(),
        UpsertTemplate {
            id: "template-1",
            title: "Delete Me",
            description: "",
            pinned: false,
            pin_order: None,
            category: None,
            targets_json: None,
            sections_json: "[]",
        },
    )
    .await
    .unwrap();

    delete_template(db.pool(), "template-1").await.unwrap();

    assert!(
        get_template(db.pool(), "template-1")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn template_insert_if_missing_preserves_existing_row() {
    let db = test_db().await;

    upsert_template(
        db.pool(),
        UpsertTemplate {
            id: "template-1",
            title: "Original",
            description: "A",
            pinned: false,
            pin_order: None,
            category: None,
            targets_json: None,
            sections_json: "[]",
        },
    )
    .await
    .unwrap();

    let inserted = insert_template_if_missing(
        db.pool(),
        UpsertTemplate {
            id: "template-1",
            title: "Replacement",
            description: "B",
            pinned: true,
            pin_order: Some(4),
            category: Some("meetings"),
            targets_json: Some("[\"exec\"]"),
            sections_json: "[{\"title\":\"Summary\",\"description\":\"Updated\"}]",
        },
    )
    .await
    .unwrap();

    assert!(!inserted);

    let row = get_template(db.pool(), "template-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.title, "Original");
    assert_eq!(row.description, "A");
    assert!(!row.pinned);
    assert_eq!(row.pin_order, None);
    assert_eq!(row.category, None);
    assert_eq!(row.targets_json, None);
    assert_eq!(row.sections_json, "[]");
}
