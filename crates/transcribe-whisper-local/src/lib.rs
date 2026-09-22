mod error;
mod service;

pub use error::*;
pub use service::*;

#[cfg(test)]
mod tests {
    use super::*;

    use axum::http::StatusCode;

    use tokio_tungstenite::{connect_async, tungstenite::Error as TungsteniteError};

    // Ersetzt die frueheren Audio-Fixtures: diese beiden Tests pruefen den
    // Fehlerpfad "Modell laedt nicht" und brauchen nur einen gueltigen
    // WAV-Rumpf, keine echte Sprache.
    fn silent_wav_16k() -> Vec<u8> {
        const SAMPLES: u32 = 1600;
        let data_len = SAMPLES * 2;
        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&16_000u32.to_le_bytes());
        wav.extend_from_slice(&32_000u32.to_le_bytes()); // byte rate
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.extend(std::iter::repeat_n(0u8, data_len as usize));
        wav
    }

    #[tokio::test]
    async fn websocket_invalid_model_path_fails_before_upgrade() {
        let app = TranscribeService::builder()
            .model_path(std::env::temp_dir().join("missing-whisper-model.bin"))
            .build()
            .into_router(|err: String| async move { (StatusCode::INTERNAL_SERVER_ERROR, err) });

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .unwrap();
        });

        let result = connect_async(format!(
            "ws://{addr}/v1/listen?channels=1&sample_rate=16000"
        ))
        .await;

        match result {
            Err(TungsteniteError::Http(response)) => {
                assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
                let body = response
                    .body()
                    .as_ref()
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                    .unwrap_or_default();
                assert!(
                    body.contains("failed to load model"),
                    "unexpected body: {body}"
                );
            }
            other => panic!("expected HTTP upgrade failure, got {other:?}"),
        }

        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn batch_invalid_model_path_returns_http_500_json_error() {
        let app = TranscribeService::builder()
            .model_path(std::env::temp_dir().join("missing-whisper-model.bin"))
            .build()
            .into_router(|err: String| async move { (StatusCode::INTERNAL_SERVER_ERROR, err) });

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .unwrap();
        });

        let response = reqwest::Client::new()
            .post(format!(
                "http://{addr}/v1/listen?channels=1&sample_rate=16000"
            ))
            .header("content-type", "audio/wav")
            .body(silent_wav_16k())
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["error"], "model_load_failed");

        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn batch_sse_invalid_model_path_returns_http_500_json_error() {
        let app = TranscribeService::builder()
            .model_path(std::env::temp_dir().join("missing-whisper-model.bin"))
            .build()
            .into_router(|err: String| async move { (StatusCode::INTERNAL_SERVER_ERROR, err) });

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .unwrap();
        });

        let response = reqwest::Client::new()
            .post(format!(
                "http://{addr}/v1/listen?channels=1&sample_rate=16000"
            ))
            .header("content-type", "audio/wav")
            .header("accept", "text/event-stream")
            .body(silent_wav_16k())
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["error"], "model_load_failed");

        let _ = shutdown_tx.send(());
    }
}
