//! Der Rot-Beweis fuer die In-Process-Tests in `src/mcp.rs` zeigt nur die
//! Faehigkeit: Server und Client teilen sich dort einen Tokio-Duplex-Kanal
//! im selben Prozess. Das beweist nie die Verdrahtung -- genau der Fehler,
//! der diese Woche schon einen Abend gekostet hat (R6b in
//! OPERATIONAL_RULES.md). Dieser Test startet das ECHTE, gebaute
//! `mitschnitt`-Binaer als eigenstaendigen Kindprozess ueber stdio, genau
//! wie Claude Desktop es nach einem Klick auf "Copy config" tun wuerde, und
//! spricht mit einem echten MCP-Client dagegen.

use std::path::PathBuf;

use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use tokio::process::Command;

#[tokio::test]
async fn the_built_binary_serves_mcp_over_stdio_against_its_own_database_path() {
    let dir = tempfile::tempdir().unwrap();
    // Im eigenen Ordner, nicht direkt im Temp-Root: seit dem Riegel aus
    // db::ensure_mcp_may_open prueft `mcp` die Ordner-Zugehoerigkeit des
    // aufgeloesten Pfads (s. den Ablehnungstest weiter unten). Ein Temp-Root
    // ohne diesen Namen waere selbst ein "fremder" Ordner und der Server
    // wuerde hier gar nicht erst starten.
    let db_path = dir.path().join("Mitschnitt").join("app.db");

    // Dieselbe Datenbank-Vorbereitung wie in den lib.rs-Tests: eine echte
    // Session anlegen, damit `list_meetings` echte Daten zurueckgibt und
    // nicht nur eine leere, aber technisch valide Antwort.
    let db = anlg_db_core::Db::connect_local_plain(&db_path)
        .await
        .unwrap();
    anlg_db_app::prepare_schema(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions (id, title, started_at) VALUES ('meeting-1', 'Planning', '2026-07-13')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    db.pool().close().await;

    let binary: PathBuf = env!("CARGO_BIN_EXE_mitschnitt").into();

    // Exakt der Weg aus db::resolve_source: `--db-path` sticht jeden Hinweis
    // und jede Bauart-Regel. Damit ist bewiesen, dass der Server dieselbe
    // Aufloesung benutzt wie jeder andere Befehl -- er bekommt hier keinen
    // eigenen, zweiten Pfad.
    let transport = TokioChildProcess::new(Command::new(&binary).configure(|cmd| {
        cmd.arg("--db-path").arg(&db_path).arg("mcp");
    }))
    .expect("den gebauten mitschnitt-Prozess ueber stdio starten");

    let client = ()
        .serve(transport)
        .await
        .expect("MCP-Handshake mit dem echten Kindprozess");

    let tools = client.list_all_tools().await.unwrap();
    let mut tool_names = tools
        .iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();
    tool_names.sort();
    assert_eq!(
        tool_names,
        [
            "decline_proposal",
            "get_meeting",
            "get_meeting_transcript",
            "get_proposal",
            "get_recurring_meeting_history",
            "list_meetings",
            "list_proposals",
            "propose_memo_edit",
            "propose_summary_edit",
        ]
    );

    // Nicht nur die Werkzeugliste: ein echter Aufruf muss die echte Zeile
    // aus der echten Datenbank ueber den echten Prozessgrenzuebergang
    // zurueckbringen.
    let result = client
        .call_tool(CallToolRequestParams::new("list_meetings"))
        .await
        .unwrap();
    let meetings = result.structured_content.unwrap();
    assert_eq!(meetings["meetings"][0]["id"], "meeting-1");
    assert_eq!(meetings["meetings"][0]["title"], "Planning");

    client.cancel().await.unwrap();
}

// Der Riegel aus db::ensure_mcp_may_open, gegen den echten, gebauten
// Binaerprozess statt gegen die Funktion allein: `mcp` bricht ab, bevor es
// eine fremde Installation auch nur oeffnet. Eine leere Attrappen-Datei
// genuegt -- der Riegel entscheidet an der Ordner-Zugehoerigkeit, nicht am
// Dateiinhalt.
#[tokio::test]
async fn the_built_binary_refuses_to_serve_mcp_from_a_foreign_installations_folder() {
    let dir = tempfile::tempdir().unwrap();
    let foreign = dir.path().join("hyprnote");
    std::fs::create_dir_all(&foreign).unwrap();
    let db_path = foreign.join("app.db");
    std::fs::write(&db_path, "").unwrap();

    let binary: PathBuf = env!("CARGO_BIN_EXE_mitschnitt").into();
    let output = Command::new(&binary)
        .arg("--db-path")
        .arg(&db_path)
        .arg("mcp")
        .output()
        .await
        .expect("den gebauten mitschnitt-Prozess starten");

    assert!(
        !output.status.success(),
        "der Server startete trotz eines Pfads in einen fremden Ordner"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("hyprnote"),
        "die Meldung nennt nicht den fremden Ordner: {stderr}"
    );
}
