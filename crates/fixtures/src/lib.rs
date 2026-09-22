//! Kleine, selbst erzeugte Testtoene fuer die Signalverarbeitung.
//!
//! Der Vorgaenger (`crates/data`) hielt 77 MB fremder Aufnahmen ohne jede
//! Herkunftsangabe. Genau deshalb ist er raus. Was hier liegt, entsteht auf dem
//! eigenen Rechner aus den Systemstimmen und ist ueber
//! `scripts/generate.sh` jederzeit neu herstellbar -- siehe `README.md`.
//!
//! Bewusst NICHT hier: alles, wofuer echte menschliche Sprache noetig waere.
//! Erkennungsguete, Sprecher-Einbettungen und Vergleiche gegen fremde
//! Referenzwerte lassen sich mit synthetischer Sprache nicht ehrlich pruefen.
//! Was diese Toene tragen, ist das Handwerk: Formate, Container, Abtastraten,
//! Kanalzahl, Blockgrenzen, Fehlerpfade.

macro_rules! fixture {
    ($name:ident, $file:literal) => {
        pub const $name: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/audio/", $file);
    };
}

// Deutsche Sprachprobe, 16 kHz, mono, 16 bit.
fixture!(SPEECH_DE_WAV, "speech-de.wav");
// Englische Sprachprobe, 16 kHz, mono, 16 bit.
fixture!(SPEECH_EN_WAV, "speech-en.wav");

// Dieselbe deutsche Quelle in den Containern, die der Einlese-Weg unterstuetzt.
fixture!(SPEECH_DE_MP3, "speech-de.mp3");
fixture!(SPEECH_DE_MP4, "speech-de.mp4");
fixture!(SPEECH_DE_M4A, "speech-de.m4a");
fixture!(SPEECH_DE_OGG, "speech-de.ogg");
fixture!(SPEECH_DE_FLAC, "speech-de.flac");
fixture!(SPEECH_DE_AAC, "speech-de.aac");
fixture!(SPEECH_DE_AIFF, "speech-de.aiff");
fixture!(SPEECH_DE_CAF, "speech-de.caf");

// Echtes Stereo mit UNTERSCHIEDLICHEN Kanaelen (links deutsch, rechts
// englisch). Zwei gleiche Kanaele wuerden auch dann noch stereo aussehen,
// wenn ein Kodierer still auf mono zusammenfaellt.
fixture!(SPEECH_STEREO_MP3, "speech-stereo.mp3");

// Abtastraten-Leiter, je 1,5 Sekunden aus derselben Quelle.
fixture!(RATE_8000_WAV, "rate-8000.wav");
fixture!(RATE_16000_WAV, "rate-16000.wav");
fixture!(RATE_22050_WAV, "rate-22050.wav");
fixture!(RATE_32000_WAV, "rate-32000.wav");
fixture!(RATE_44100_WAV, "rate-44100.wav");
fixture!(RATE_48000_WAV, "rate-48000.wav");

/// Jede Datei, die dieses Crate anbietet -- fuer den Vollstaendigkeitstest.
pub const ALL: &[&str] = &[
    SPEECH_DE_WAV,
    SPEECH_EN_WAV,
    SPEECH_DE_MP3,
    SPEECH_DE_MP4,
    SPEECH_DE_M4A,
    SPEECH_DE_OGG,
    SPEECH_DE_FLAC,
    SPEECH_DE_AAC,
    SPEECH_DE_AIFF,
    SPEECH_DE_CAF,
    SPEECH_STEREO_MP3,
    RATE_8000_WAV,
    RATE_16000_WAV,
    RATE_22050_WAV,
    RATE_32000_WAV,
    RATE_44100_WAV,
    RATE_48000_WAV,
];

#[cfg(test)]
mod tests {
    use super::*;

    // Ein Pfad, der auf nichts zeigt, ist der teuerste Ausgang: die Tests, die
    // ihn benutzen, scheitern dann an der fehlenden Datei statt an der Sache.
    #[test]
    fn every_advertised_file_exists_and_is_not_empty() {
        for path in ALL {
            let size = std::fs::metadata(path)
                .unwrap_or_else(|error| panic!("{path}: {error}"))
                .len();
            assert!(size > 1024, "{path} ist mit {size} Bytes zu klein");
        }
    }

    // Das Platzbudget ist der Grund, warum dieses Crate existiert. Ohne
    // Obergrenze waechst es zurueck in die 77 MB, die wir gerade losgeworden
    // sind -- also wird sie gemessen und nicht nur aufgeschrieben.
    #[test]
    fn the_whole_set_stays_far_below_five_megabytes() {
        let total: u64 = ALL
            .iter()
            .map(|path| std::fs::metadata(path).unwrap().len())
            .sum();

        assert!(
            total < 5 * 1024 * 1024,
            "die Testtoene sind auf {total} Bytes gewachsen"
        );
    }

    #[test]
    fn the_two_speech_samples_are_mono_sixteen_kilohertz() {
        for path in [SPEECH_DE_WAV, SPEECH_EN_WAV] {
            let spec = hound::WavReader::open(path).unwrap().spec();
            assert_eq!(spec.channels, 1, "{path}");
            assert_eq!(spec.sample_rate, 16_000, "{path}");
        }
    }

    #[test]
    fn the_rate_ladder_really_carries_the_rates_in_its_names() {
        for (path, expected) in [
            (RATE_8000_WAV, 8_000),
            (RATE_16000_WAV, 16_000),
            (RATE_22050_WAV, 22_050),
            (RATE_32000_WAV, 32_000),
            (RATE_44100_WAV, 44_100),
            (RATE_48000_WAV, 48_000),
        ] {
            let spec = hound::WavReader::open(path).unwrap().spec();
            assert_eq!(spec.sample_rate, expected, "{path}");
            assert_eq!(spec.channels, 1, "{path}");
        }
    }
}
