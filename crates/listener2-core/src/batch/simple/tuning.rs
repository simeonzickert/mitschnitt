//! Stellschrauben der lokalen Soniqo-Strecke -- ausschliesslich fuer die
//! Messbank (`examples/bench_soniqo.rs`, Testplan 02.09.2026, Stufe 0).
//!
//! Warum ein prozessweiter Wert und kein durchgereichter Parameter: die
//! Konfiguration muesste sonst durch `run_batch` und damit durch die
//! oeffentliche Schnittstelle des Tauri-Plugins wandern. Das waere ein Eingriff
//! in Produktivcode fuer ein Werkzeug, das nur im Beispiel-Binary laeuft.
//!
//! Zusicherung, die die Tests in `simple/tests.rs` festhalten: ohne einen
//! einzigen Aufruf von [`set_soniqo_tuning`] liefert jede Funktion hier exakt
//! die Werte, die vor dieser Datei fest im Code standen. Die App setzt nichts
//! -- sie ruft die Setter nie auf.

use std::sync::RwLock;

use anlg_transcribe_core::TARGET_SAMPLE_RATE;

/// Wie ein Kanal in Modell-Eingaben zerlegt wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoniqoChunkingMode {
    /// Starre Fenster fester Laenge (anarlogs Weg, Upstream-Verhalten).
    Fixed { max_samples: usize },
    /// Schnitte an Sprechpausen (heutiger Standard des Forks).
    SpeechAware,
}

/// Die Stellschrauben. `PRODUCTION` ist der heutige Stand, Feld fuer Feld.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoniqoTuning {
    /// Zerlegungsart.
    pub chunking: SoniqoChunkingMode,
    /// Zielgroesse eines Buendels in Abtastwerten. `None` = das Modellfenster
    /// (29,5 s), also der heutige Wert. `Some(0)` heisst: nicht buendeln, jeder
    /// Sprech-Abschnitt geht allein raus.
    pub bundle_max_samples: Option<usize>,
    /// Erlaubte ZEITSPANNE eines Buendels in Abtastwerten -- vom Sprechbeginn
    /// des ersten bis zum Sprechende des letzten Mitglieds, verworfene Stille
    /// eingerechnet. `None` faellt auf
    /// [`super::local::SONIQO_BUNDLE_MAX_SPAN_SAMPLES`] zurueck -- denselben
    /// Wert, den `PRODUCTION` ausdruecklich traegt.
    pub bundle_max_span_samples: Option<usize>,
    /// Schwelle des Stille-Gates auf dem Mikrofonkanal. `None` = Gate aus.
    pub direct_mic_min_rms: Option<f64>,
    /// Kanaele gleichzeitig statt nacheinander.
    pub channels_in_parallel: bool,
    /// Fork (02.09.2026): die GEMESSENEN Wortzeiten des Erkenners benutzen,
    /// statt die Woerter gleichmaessig ueber den Abschnitt zu verteilen.
    ///
    /// Nur der Nachbearbeitungsweg. Der Live-Weg interpoliert weiter -- das ist
    /// kein Widerspruch mehr: der Abgleich in
    /// `apps/desktop/src/stt/live-segment.ts` stand bis zum Abend des
    /// 02.09.2026 allein auf 80 % zeitlicher Ueberlappung und liess deshalb
    /// dasselbe Wort zweimal stehen, sobald die beiden Seiten unterschiedlich
    /// genaue Zeiten trugen. Er steht jetzt zusaetzlich auf Text, Reihenfolge
    /// und einer Eins-zu-eins-Zuordnung.
    pub word_timings: bool,
}

impl SoniqoTuning {
    /// Der Zustand, in dem die App laeuft.
    ///
    /// `channels_in_parallel: true` seit dem 02.09.2026: die Messreihe an
    /// des Betreibers echtem Gespraech (23,6 min, zwei Kanaele) hat Achse D des
    /// Testplans entschieden. Wanduhr 15,7 s nacheinander gegen 9,6 s
    /// gleichzeitig, Transkript Wort fuer Wort identisch (3393 Woerter,
    /// 37 Namenstreffer, in beiden Laeufen dieselbe eine Halluzination).
    /// Gebuendelt wird weiter auf das Modellfenster (29,5 s) -- daran aendert
    /// dieser Entscheid nichts.
    ///
    /// `bundle_max_span_samples` deckelt die ZEITSPANNE eines Buendels auf
    /// dieselben 29,5 s. Ohne diesen zweiten Deckel waere sie unbegrenzt, weil
    /// die sprachbewusste Zerlegung die Stille dazwischen verwirft -- ein Wort
    /// koennte dann beliebig weit springen.
    ///
    /// `word_timings: true` seit dem Abend des 02.09.2026. Gemessen an
    /// `tom.mp3` gegen den ungebuendelten Lauf: Versatz im Mittel 0,918 ->
    /// 0,345 s, Median 0,367 -> 0,214 s, p95 3,436 -> 0,873 s, Stellen ueber
    /// zwei Sekunden 250 -> 22. Wortzahl unveraendert 3355, Wanduhr 13,2 ->
    /// 13,2 s. Der Rueckfall auf das Schaetzen bleibt Wort fuer Wort moeglich
    /// und meldet sich, wenn er greift.
    ///
    /// Die Messbank kann mit `--channels sequential` weiter nacheinander
    /// messen; die Strecke dafuer bleibt im Code und ist eigens getestet.
    pub const PRODUCTION: Self = Self {
        chunking: SoniqoChunkingMode::SpeechAware,
        bundle_max_samples: None,
        bundle_max_span_samples: Some(super::local::SONIQO_BUNDLE_MAX_SPAN_SAMPLES),
        direct_mic_min_rms: Some(super::local::SONIQO_DIRECT_MIC_MIN_RMS),
        channels_in_parallel: true,
        word_timings: true,
    };

    /// Gemessene Wortzeiten an- oder abschalten.
    pub const fn with_word_timings(mut self, enabled: bool) -> Self {
        self.word_timings = enabled;
        self
    }

    /// Zielgroesse eines Buendels in Sekunden setzen. `0.0` = nicht buendeln.
    pub fn with_bundle_seconds(mut self, seconds: f64) -> Self {
        self.bundle_max_samples = Some(seconds_to_samples(seconds));
        self
    }

    /// Erlaubte Zeitspanne eines Buendels in Sekunden setzen. `0.0` bedeutet:
    /// kein Buendel mit mehr als einem Mitglied, denn schon das zweite
    /// Mitglied sprengt die Spanne.
    pub fn with_bundle_span_seconds(mut self, seconds: f64) -> Self {
        self.bundle_max_span_samples = Some(seconds_to_samples(seconds));
        self
    }

    /// Starre Fenster mit der angegebenen Laenge.
    pub fn with_fixed_chunking(mut self, seconds: f64) -> Self {
        self.chunking = SoniqoChunkingMode::Fixed {
            max_samples: seconds_to_samples(seconds).max(1),
        };
        self
    }
}

impl Default for SoniqoTuning {
    fn default() -> Self {
        Self::PRODUCTION
    }
}

pub fn seconds_to_samples(seconds: f64) -> usize {
    if seconds <= 0.0 {
        return 0;
    }
    (seconds * f64::from(TARGET_SAMPLE_RATE)).round() as usize
}

static TUNING: RwLock<SoniqoTuning> = RwLock::new(SoniqoTuning::PRODUCTION);

/// Setzt die Stellschrauben prozessweit. Nur die Messbank ruft das auf.
pub fn set_soniqo_tuning(tuning: SoniqoTuning) {
    let mut guard = TUNING.write().unwrap_or_else(|error| error.into_inner());
    *guard = tuning;
}

/// Liest die Stellschrauben. Ohne vorheriges Setzen: [`SoniqoTuning::PRODUCTION`].
pub fn soniqo_tuning() -> SoniqoTuning {
    *TUNING.read().unwrap_or_else(|error| error.into_inner())
}
