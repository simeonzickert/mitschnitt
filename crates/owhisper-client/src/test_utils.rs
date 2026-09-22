use crate::RealtimeSttAdapter;

pub struct UrlTestCase {
    pub name: &'static str,
    pub model: Option<&'static str>,
    pub languages: &'static [anlg_language::ISO639],
    pub contains: &'static [&'static str],
    pub not_contains: &'static [&'static str],
}

pub fn run_url_test_cases<A: RealtimeSttAdapter>(
    adapter: &A,
    api_base: &str,
    cases: &[UrlTestCase],
) {
    for case in cases {
        let params = owhisper_interface::ListenParams {
            model: case.model.map(str::to_string),
            languages: case.languages.iter().map(|l| (*l).into()).collect(),
            ..Default::default()
        };

        let url = adapter.build_ws_url(api_base, &params, 1);
        let url_str = url.as_str();

        for expected in case.contains {
            assert!(
                url_str.contains(expected),
                "[{}] URL should contain '{}' but got: {}",
                case.name,
                expected,
                url_str
            );
        }
        for unexpected in case.not_contains {
            assert!(
                !url_str.contains(unexpected),
                "[{}] URL should NOT contain '{}' but got: {}",
                case.name,
                unexpected,
                url_str
            );
        }
    }
}

/// Ersatz fuer die frueheren Audio-Fixtures aus `anlg-data`: eine echte,
/// gueltige WAV-Datei (16 kHz, mono, 16 bit, eine Sekunde Sinus). Die Tests,
/// die sie benutzen, pruefen Transportvertraege - Endpunkt, Header, Body-Laenge -
/// und brauchen dafuer kein Sprachmaterial.
pub fn sample_wav_file() -> tempfile::NamedTempFile {
    const SAMPLE_RATE: u32 = 16_000;
    const SAMPLES: u32 = SAMPLE_RATE;
    let data_len = SAMPLES * 2;

    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for n in 0..SAMPLES {
        let t = n as f32 / SAMPLE_RATE as f32;
        let sample = ((t * 440.0 * std::f32::consts::TAU).sin() * 8000.0) as i16;
        wav.extend_from_slice(&sample.to_le_bytes());
    }

    let mut file = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile()
        .expect("create temp wav");
    std::io::Write::write_all(&mut file, &wav).expect("write temp wav");
    std::io::Write::flush(&mut file).expect("flush temp wav");
    file
}
