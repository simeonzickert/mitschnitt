// Fork-Patch (02.09.2026): echte Wortzeiten aus dem Parakeet-Erkenner holen.
//
// Warum ueberhaupt: das Modell liefert einen flachen Text ohne Zeiten, und
// unsere Strecke verteilt die Woerter danach gleichmaessig ueber den
// Tonabschnitt, der hineinging. Bei kurzen Abschnitten faellt das nicht auf,
// bei gebuendelten bis zu 18 Sekunden. Die Zeit ist im Erkenner aber
// vorhanden: `TDTGreedyDecoder` gibt jedes Token an einem konkreten
// Encoder-Frame aus (`t`), und der Dauer-Kopf sagt zusaetzlich, wie viele
// Frames es abdeckt. Beides wird heute schlicht nicht mitgeschrieben.
//
// Umgehen laesst sich das nicht: `melPreprocessor`, `vocabulary`, `decoder`
// und `joint` sind in `ParakeetASRModel` privat, `MelPreprocessor` ist
// `internal`. Ein Patch im Bau ist der Weg -- derselbe Mechanismus, den
// `streaming_offline.rs` seit dem Fork-Beginn benutzt.
//
// Die Anker sind bewusst lang und wortgetreu. Sitzt einer nach einem
// `speech-swift`-Sprung nicht mehr, BRICHT DER BAU LAUT (build.rs paniziert
// mit dem jeweiligen Fehlertext), statt still falsche Zeiten zu rechnen. Das
// ist die Eigenschaft, die den Patch vertretbar macht.
//
// Aufloesung der Zeitachse: `hopLength 160` bei 16 kHz sind 10 ms je
// Mel-Frame, `subsamplingFactor 8` macht daraus 80 ms je Encoder-Frame. Die
// Umrechnung passiert in Swift aus der Konfiguration, nicht aus einer hier
// eingetragenen Zahl.

/// Wendet einen Patch-Schritt an -- und bricht laut, wenn der Zustand nicht
/// eindeutig ist.
///
/// Bis zum 02.09.2026 galt ein Schritt als erledigt, sobald seine Ersetzung
/// IRGENDWO in der Datei vorkam. Das ist zu wenig: liegt neben einer bereits
/// gepatchten Stelle noch eine ungepatchte (ein aelterer oder von Hand
/// angewandter Patch, eine zweite Methode mit demselben Aufruf), wird der
/// Schritt uebersprungen, der Bau gelingt -- und die tatsaechlich ausgefuehrte
/// Methode setzt nie Zeiten. Genau der Ausgang, den der Vertrag dieses Moduls
/// ausschliesst: bricht laut, statt still falsch zu rechnen.
///
/// Deshalb wird BEIDES gezaehlt. Anker, die innerhalb einer Ersetzung stehen
/// (mehrere Ersetzungen enthalten ihren eigenen Anker als Teilstueck), zaehlen
/// dabei nicht mit -- sonst waere jeder gepatchte Zustand mehrdeutig.
/// Erlaubt sind genau zwei Bilder: unberuehrt (ein Anker, keine Ersetzung) und
/// fertig (keine Anker, genau eine Ersetzung). Alles andere ist ein Fehler.
fn apply_patch_step<'a>(
    source: &str,
    anchor: &str,
    replacement: &str,
    was_patched_before: bool,
    missing: &'a str,
) -> Result<String, &'a str> {
    let replacements = source.matches(replacement).count();
    let anchors = anchors_outside_replacements(source, anchor, replacement);

    match (replacements, anchors) {
        (1, 0) => Ok(source.to_string()),
        // "Unberuehrt" -- aber nur, wenn die Datei ueberhaupt unberuehrt war.
        // Trug sie beim Eintritt schon unseren Fingerabdruck, ist dieses Bild
        // eine Luege: dann steht hier eine ANDERE Fassung unseres Patches,
        // deren Ersetzung den Anker als Praefix enthaelt. Ein zweiter
        // Durchlauf wuerde sie nicht ersetzen, sondern daneben eine zweite
        // anlegen.
        //
        // `was_patched_before` kommt von aussen und wird EINMAL an der
        // unberuehrten Quelle bestimmt. Es hier selbst zu messen waere falsch:
        // die frueheren Schritte desselben Laufs schreiben den Fingerabdruck ja
        // gerade erst hinein, und der zweite Schritt haelte den ersten fuer
        // eine fremde Fassung.
        (0, 1) if was_patched_before => Err(FOREIGN_PATCH_VERSION),
        (0, 1) => Ok(source.replacen(anchor, replacement, 1)),
        (0, 0) => Err(missing),
        _ => Err(AMBIGUOUS_PATCH_STATE),
    }
}

/// Wie oft steht der Anker AUSSERHALB einer bereits gepatchten Stelle? Die
/// Ersetzungen werden vorher durch ein Zeichen ersetzt, das in Quelltext nicht
/// vorkommt -- so kann beim Herausschneiden auch kein neuer Anker entstehen.
fn anchors_outside_replacements(source: &str, anchor: &str, replacement: &str) -> usize {
    source.replace(replacement, "\u{0}").matches(anchor).count()
}

const AMBIGUOUS_PATCH_STATE: &str = concat!(
    "Fork-Wortzeiten-Patch: die Quelle ist weder unberuehrt noch sauber gepatcht ",
    "(Anker und Ersetzung stehen nebeneinander, oder eine von beiden mehrfach). ",
    "Den vendorierten speech-swift-Checkout loeschen und neu bauen."
);

const FOREIGN_PATCH_VERSION: &str = concat!(
    "Fork-Wortzeiten-Patch: der Checkout traegt eine ANDERE Fassung dieses ",
    "Patches. Ein zweiter Durchlauf wuerde sie nicht ersetzen, sondern daneben ",
    "eine zweite anlegen (doppelte Eigenschaft, doppelter Aufruf) -- der Bau ",
    "faende das erst Minuten spaeter als Swift-Fehler. Den vendorierten ",
    "checkouts/speech-swift-Ordner loeschen und neu bauen."
);

/// Der Fingerabdruck einer Datei: eine Zeichenkette, die JEDE Fassung dieses
/// Patches hineinschreibt und die in keiner unberuehrten Quelle vorkommt.
///
/// Wofuer, denn der Zaehl-Vergleich in [`apply_patch_step`] sieht auf den
/// ersten Blick vollstaendig aus: mehrere unserer Ersetzungen ENTHALTEN ihren
/// eigenen Anker als Praefix. `PATCHED_PROPERTY` beginnt woertlich mit
/// `PROPERTY`, `PATCHED_WORDS` mit `WORDS`. Steht im Checkout eine AELTERE
/// Fassung unserer Ersetzung, dann zaehlt die heutige Ersetzung null Treffer,
/// der Anker steht aber noch da -- das Bild `(0, 1)`, das "unberuehrt" heisst.
/// Der Schritt patcht ein zweites Mal, und die Datei traegt danach beide
/// Fassungen: doppelte Eigenschaft, doppelter Aufruf, Swift-Fehler nach Minuten
/// Bauzeit.
///
/// Gemessen am 03.09.2026: genau so steht es im vendorierten Checkout dieses
/// Baums (Stand `26d82f871f`, Swift-Bau vom 02.09. 19:59) fuer
/// `ParakeetASR.swift` und `Vocabulary.swift`. `TDTGreedyDecoder.swift` ist
/// zwischen den beiden Staenden unveraendert und deshalb nicht betroffen.
///
/// Der Fingerabdruck haengt bewusst NICHT an einer einzelnen Ersetzung. Eine
/// frueherere Fassung dieses Waechters pruefte je Datei EINEN Marker aus EINEM
/// der sechs Schritte -- eine kuenftige Fassung, die nur einen anderen Schritt
/// aendert, waere daran vorbeigelaufen. Deshalb prueft jetzt jeder Schritt
/// selbst, und zwar an der einzigen Stelle, an der es darauf ankommt: dort, wo
/// er "unberuehrt" schliessen wuerde.

/// Der Dekoder schreibt zu jedem ausgegebenen Token seinen Encoder-Frame und
/// das Frame-Ende (Frame + vorhergesagte Dauer) mit.
///
/// Drei Anker. Der mittlere zieht die Dauer-Berechnung VOR die Ausgabe des
/// Tokens -- ohne sie ist das Frame-Ende an dieser Stelle nicht bekannt. Am
/// Ablauf aendert das nichts: `argmax(durationLogits, floatBuf: nil)` nimmt den
/// skalaren Pfad und fasst den geteilten `argmaxBuf` nicht an.
pub(crate) fn patch_tdt_greedy_decoder_source(source: &str) -> Result<String, &'static str> {
    const SIGNATURE: &str = "    func decode(encoded: MLMultiArray, encodedLength: Int) throws -> (tokens: [Int], tokenLogProbs: [Float], confidence: Float) {\n        var tokens = [Int]()\n        var tokenLogProbs = [Float]()\n";
    const PATCHED_SIGNATURE: &str = "    func decode(encoded: MLMultiArray, encodedLength: Int) throws -> (tokens: [Int], tokenLogProbs: [Float], confidence: Float, tokenFrameStarts: [Int], tokenFrameEnds: [Int]) {\n        var tokens = [Int]()\n        var tokenLogProbs = [Float]()\n        var tokenFrameStarts = [Int]()\n        var tokenFrameEnds = [Int]()\n";

    const EMIT: &str = "            } else {\n                if tokenId >= firstTextTokenId {\n                    tokens.append(tokenId)\n                    // Compute log-softmax: log_prob = logit[id] - log(sum(exp(logits)))\n                    let logProb = logSoftmax(tokenLogits, tokenId: tokenId, count: config.vocabSize + 1, floatBuf: argmaxBuf)\n                    tokenLogProbs.append(logProb)\n                }\n\n                let durationIdx = argmax(durationLogits, count: config.numDurationBins, floatBuf: nil)\n                let duration = config.durationBins[durationIdx]\n                t += max(duration, 1)\n";
    const PATCHED_EMIT: &str = "            } else {\n                let durationIdx = argmax(durationLogits, count: config.numDurationBins, floatBuf: nil)\n                let duration = config.durationBins[durationIdx]\n                let advance = max(duration, 1)\n\n                if tokenId >= firstTextTokenId {\n                    tokens.append(tokenId)\n                    // Fork: der Frame, an dem dieses Token entstanden ist, und das\n                    // Frame-Ende aus dem Dauer-Kopf. Genau diese beiden Zahlen\n                    // fehlten -- alles andere stand hier schon.\n                    tokenFrameStarts.append(t)\n                    tokenFrameEnds.append(t + advance)\n                    // Compute log-softmax: log_prob = logit[id] - log(sum(exp(logits)))\n                    let logProb = logSoftmax(tokenLogits, tokenId: tokenId, count: config.vocabSize + 1, floatBuf: argmaxBuf)\n                    tokenLogProbs.append(logProb)\n                }\n\n                t += advance\n";

    const RETURN: &str = "        return (tokens, tokenLogProbs, confidence)\n";
    const PATCHED_RETURN: &str =
        "        return (tokens, tokenLogProbs, confidence, tokenFrameStarts, tokenFrameEnds)\n";

    let was_patched_before = source.contains("tokenFrameStarts");

    let patched = apply_patch_step(
        source,
        SIGNATURE,
        PATCHED_SIGNATURE,
        was_patched_before,
        "TDTGreedyDecoder decode signature not found",
    )?;
    let patched = apply_patch_step(
        &patched,
        EMIT,
        PATCHED_EMIT,
        was_patched_before,
        "TDTGreedyDecoder token emission block not found",
    )?;
    let patched = apply_patch_step(
        &patched,
        RETURN,
        PATCHED_RETURN,
        was_patched_before,
        "TDTGreedyDecoder decode return not found",
    )?;

    Ok(patched)
}

/// `ParakeetASRModel` reicht die Frames durch und legt daraus die Wortzeiten
/// ab. Fuenf Anker.
///
/// Der Fensterversatz ist der Teil, den man leicht uebersieht: Ton laenger als
/// das Encoder-Fenster wird in Fenster zerlegt, und jedes Fenster zaehlt seine
/// Frames wieder ab null. Ohne `frameOffset` laege jedes Wort ab dem zweiten
/// Fenster am Anfang der Aufnahme.
pub(crate) fn patch_parakeet_asr_source(source: &str) -> Result<String, &'static str> {
    const PROPERTY: &str = "    /// Per-word confidence scores from the last transcription.\n    public private(set) var lastWordConfidences: [WordConfidence]?\n";
    const PATCHED_PROPERTY: &str = "    /// Per-word confidence scores from the last transcription.\n    public private(set) var lastWordConfidences: [WordConfidence]?\n    /// Fork: Wortzeiten UND Wortsicherheit der letzten Transkription, aus den\n    /// Encoder-Frames und den Log-Wahrscheinlichkeiten des Dekoders. `nil`,\n    /// wenn eine der drei Listen nicht zur Token-Liste passt.\n    public private(set) var lastWordTimings: [ParakeetVocabulary.WordTiming]?\n";

    const ACCUMULATORS: &str =
        "        var tokenIds: [Int] = []\n        var tokenLogProbs: [Float] = []\n";
    const PATCHED_ACCUMULATORS: &str = "        var tokenIds: [Int] = []\n        var tokenLogProbs: [Float] = []\n        var tokenFrameStarts: [Int] = []\n        var tokenFrameEnds: [Int] = []\n";

    const SINGLE_WINDOW: &str = "            let r = try encodeAndDecodeWindow(mel: mel, actualLength: melLength, maskedTokenIds: masked)\n            tokenIds = r.tokens; tokenLogProbs = r.tokenLogProbs\n";
    const PATCHED_SINGLE_WINDOW: &str = "            let r = try encodeAndDecodeWindow(mel: mel, actualLength: melLength, maskedTokenIds: masked)\n            tokenIds = r.tokens; tokenLogProbs = r.tokenLogProbs\n            tokenFrameStarts = r.tokenFrameStarts; tokenFrameEnds = r.tokenFrameEnds\n";

    const MULTI_WINDOW: &str = "                let r = try encodeAndDecodeWindow(mel: windowMel, actualLength: win, maskedTokenIds: masked)\n                tokenIds += r.tokens; tokenLogProbs += r.tokenLogProbs\n                start += maxWindow; windows += 1\n";
    const PATCHED_MULTI_WINDOW: &str = "                let r = try encodeAndDecodeWindow(mel: windowMel, actualLength: win, maskedTokenIds: masked)\n                tokenIds += r.tokens; tokenLogProbs += r.tokenLogProbs\n                // Fork: jedes Fenster zaehlt seine Encoder-Frames ab null. Ohne\n                // diesen Versatz laege alles ab dem zweiten Fenster am Anfang.\n                let frameOffset = start / config.subsamplingFactor\n                tokenFrameStarts += r.tokenFrameStarts.map { $0 + frameOffset }\n                tokenFrameEnds += r.tokenFrameEnds.map { $0 + frameOffset }\n                start += maxWindow; windows += 1\n";

    const WINDOW_SIGNATURE: &str = "    private func encodeAndDecodeWindow(mel: MLMultiArray, actualLength: Int, maskedTokenIds: Set<Int> = [])\n        throws -> (tokens: [Int], tokenLogProbs: [Float], confidence: Float)\n";
    const PATCHED_WINDOW_SIGNATURE: &str = "    private func encodeAndDecodeWindow(mel: MLMultiArray, actualLength: Int, maskedTokenIds: Set<Int> = [])\n        throws -> (tokens: [Int], tokenLogProbs: [Float], confidence: Float, tokenFrameStarts: [Int], tokenFrameEnds: [Int])\n";

    const WORDS: &str =
        "        lastWordConfidences = vocabulary.decodeWords(tokenIds, logProbs: tokenLogProbs)\n";
    const PATCHED_WORDS: &str = "        lastWordConfidences = vocabulary.decodeWords(tokenIds, logProbs: tokenLogProbs)\n        lastWordTimings = vocabulary.decodeWordTimings(\n            tokenIds,\n            logProbs: tokenLogProbs,\n            frameStarts: tokenFrameStarts,\n            frameEnds: tokenFrameEnds,\n            secondsPerFrame: Float(config.hopLength * config.subsamplingFactor)\n                / Float(config.sampleRate))\n";

    let steps: [(&str, &str, &'static str); 6] = [
        (
            PROPERTY,
            PATCHED_PROPERTY,
            "ParakeetASR lastWordConfidences property not found",
        ),
        (
            ACCUMULATORS,
            PATCHED_ACCUMULATORS,
            "ParakeetASR token accumulators not found",
        ),
        (
            SINGLE_WINDOW,
            PATCHED_SINGLE_WINDOW,
            "ParakeetASR single-window decode not found",
        ),
        (
            MULTI_WINDOW,
            PATCHED_MULTI_WINDOW,
            "ParakeetASR multi-window decode not found",
        ),
        (
            WINDOW_SIGNATURE,
            PATCHED_WINDOW_SIGNATURE,
            "ParakeetASR encodeAndDecodeWindow signature not found",
        ),
        (
            WORDS,
            PATCHED_WORDS,
            "ParakeetASR decodeWords call not found",
        ),
    ];

    let was_patched_before = source.contains("lastWordTimings");

    let mut patched = source.to_string();
    for (anchor, replacement, error) in steps {
        patched = apply_patch_step(&patched, anchor, replacement, was_patched_before, error)?;
    }

    Ok(patched)
}

/// Die Wortgrenzen kennt nur das Woerterbuch (SentencePiece-Marke `U+2581`).
/// Der Patch haengt eine zweite Methode neben `decodeWords` -- dieselbe
/// Gruppierung, statt Vertrauenswerten die Frames.
pub(crate) fn patch_parakeet_vocabulary_source(source: &str) -> Result<String, &'static str> {
    const ANCHOR: &str = "    /// Decode token IDs into words with per-word confidence scores.\n";
    const ADDITION: &str = r#"    /// Fork: ein Wort mit seiner gemessenen Zeit UND seiner Sicherheit.
    ///
    /// `AudioCommon.AlignedWord` traegt nur Text und Zeit. Die Sicherheit
    /// daneben in einer zweiten Liste zu fuehren waere eine Parallelfuehrung,
    /// die beim ersten Umbau der Gruppierung still auseinanderlaeuft -- hier
    /// haelt der Typ die drei Angaben zusammen.
    public struct WordTiming: Sendable {
        public let text: String
        public let startTime: Float
        public let endTime: Float
        /// exp(Mittel der Log-Wahrscheinlichkeiten der Tokens dieses Wortes),
        /// auf 0..1 gedeckelt -- dieselbe Rechnung wie in `decodeWords`.
        public let confidence: Float

        public init(text: String, startTime: Float, endTime: Float, confidence: Float) {
            self.text = text
            self.startTime = startTime
            self.endTime = endTime
            self.confidence = confidence
        }
    }

    /// Fork: Woerter mit den Zeiten, an denen der Dekoder sie ausgegeben hat,
    /// und mit der Sicherheit, die er ihnen gegeben hat.
    ///
    /// Gruppiert wie `decodeWords` an der SentencePiece-Marke, nimmt aber je
    /// Wort den Frame des ersten und das Frame-Ende des letzten Tokens.
    /// Liefert `nil`, wenn die Frame- oder Wahrscheinlichkeitslisten nicht zur
    /// Token-Liste passen -- eine falsche Zeit waere schlimmer als gar keine.
    public func decodeWordTimings(
        _ tokenIds: [Int],
        logProbs: [Float],
        frameStarts: [Int],
        frameEnds: [Int],
        secondsPerFrame: Float
    ) -> [WordTiming]? {
        guard tokenIds.count == frameStarts.count, tokenIds.count == frameEnds.count,
            tokenIds.count == logProbs.count
        else {
            return nil
        }

        var words = [WordTiming]()
        var currentWord = ""
        var currentStart = 0
        var currentEnd = 0
        var currentLogProbs = [Float]()

        func flush() {
            let meanLP = currentLogProbs.reduce(0, +) / Float(currentLogProbs.count)
            words.append(
                WordTiming(
                    text: currentWord,
                    startTime: Float(currentStart) * secondsPerFrame,
                    endTime: Float(currentEnd) * secondsPerFrame,
                    confidence: min(1.0, exp(meanLP))))
        }

        for (i, id) in tokenIds.enumerated() {
            guard let token = idToToken[id] else { continue }

            let startsNewWord = token.hasPrefix("\u{2581}")
            let text = token.replacingOccurrences(of: "\u{2581}", with: "")

            if startsNewWord && !currentWord.isEmpty {
                flush()
                currentWord = ""
                currentLogProbs = []
            }

            if currentWord.isEmpty {
                currentStart = frameStarts[i]
                currentEnd = frameEnds[i]
            } else {
                currentEnd = max(currentEnd, frameEnds[i])
            }

            currentWord += text
            currentLogProbs.append(logProbs[i])
        }

        if !currentWord.isEmpty {
            flush()
        }

        return words
    }

"#;

    // Frueher stand hier nur der MARKER -- allein der Name `decodeWordTimings`
    // genuegte als Beleg, dass der Schritt erledigt ist. Rumpf und
    // Rueckgabesemantik wurden nie geprueft, eine fremde Methode gleichen
    // Namens haette den Patch stillschweigend verhindert. Jetzt entscheidet der
    // vollstaendige Text.
    // Fingerabdruck ist "Fork:" und NICHT der Methodenname `decodeWordTimings`:
    // eine fremde Methode gleichen Namens waere sonst als unsere aeltere
    // Fassung durchgegangen und haette den Bau abgebrochen, statt zu patchen
    // (der Test `a_foreign_method_of_the_same_name_does_not_count_as_patched`
    // hat genau das gefangen). Kein Kommentar dieses Patches kommt ohne
    // "Fork:" aus, und keine der drei unberuehrten Quellen traegt die
    // Zeichenkette -- gegen die gepinnte speech-swift-Revision gemessen.
    let was_patched_before = source.contains("Fork:");

    apply_patch_step(
        source,
        ANCHOR,
        &format!("{ADDITION}{ANCHOR}"),
        was_patched_before,
        "ParakeetVocabulary decodeWords documentation anchor not found",
    )
}

#[cfg(test)]
mod tests {
    use super::{
        FOREIGN_PATCH_VERSION, patch_parakeet_asr_source, patch_parakeet_vocabulary_source,
        patch_tdt_greedy_decoder_source,
    };

    const DECODER: &str = r#"struct TDTGreedyDecoder {
    func decode(encoded: MLMultiArray, encodedLength: Int) throws -> (tokens: [Int], tokenLogProbs: [Float], confidence: Float) {
        var tokens = [Int]()
        var tokenLogProbs = [Float]()
        var t = 0
        while t < encodedLength {
            if tokenId == config.blankTokenId {
                t += 1
            } else {
                if tokenId >= firstTextTokenId {
                    tokens.append(tokenId)
                    // Compute log-softmax: log_prob = logit[id] - log(sum(exp(logits)))
                    let logProb = logSoftmax(tokenLogits, tokenId: tokenId, count: config.vocabSize + 1, floatBuf: argmaxBuf)
                    tokenLogProbs.append(logProb)
                }

                let durationIdx = argmax(durationLogits, count: config.numDurationBins, floatBuf: nil)
                let duration = config.durationBins[durationIdx]
                t += max(duration, 1)
            }
        }
        return (tokens, tokenLogProbs, confidence)
    }
}
"#;

    const ASR: &str = r#"public class ParakeetASRModel {
    /// Per-word confidence scores from the last transcription.
    public private(set) var lastWordConfidences: [WordConfidence]?

    public func transcribeAudio(_ audio: [Float], sampleRate: Int, language: String? = nil) throws -> String {
        var tokenIds: [Int] = []
        var tokenLogProbs: [Float] = []

        if melLength <= maxWindow {
            let r = try encodeAndDecodeWindow(mel: mel, actualLength: melLength, maskedTokenIds: masked)
            tokenIds = r.tokens; tokenLogProbs = r.tokenLogProbs
        } else {
            while start < melLength {
                let r = try encodeAndDecodeWindow(mel: windowMel, actualLength: win, maskedTokenIds: masked)
                tokenIds += r.tokens; tokenLogProbs += r.tokenLogProbs
                start += maxWindow; windows += 1
            }
        }

        let text = vocabulary.decode(tokenIds)
        lastWordConfidences = vocabulary.decodeWords(tokenIds, logProbs: tokenLogProbs)
        return text
    }

    private func encodeAndDecodeWindow(mel: MLMultiArray, actualLength: Int, maskedTokenIds: Set<Int> = [])
        throws -> (tokens: [Int], tokenLogProbs: [Float], confidence: Float)
    {
        return try tdtDecoder.decode(encoded: encoded, encodedLength: encodedLength)
    }
}
"#;

    const VOCABULARY: &str = r#"public struct ParakeetVocabulary: Sendable {
    private let idToToken: [Int: String]

    /// Decode token IDs into words with per-word confidence scores.
    public func decodeWords(_ tokenIds: [Int], logProbs: [Float]) -> [WordConfidence] {
        return []
    }
}
"#;

    #[test]
    fn decoder_records_frame_start_and_end_per_token() {
        let patched = patch_tdt_greedy_decoder_source(DECODER).unwrap();

        assert!(patched.contains("tokenFrameStarts.append(t)"));
        assert!(patched.contains("tokenFrameEnds.append(t + advance)"));
        assert!(patched.contains(
            "return (tokens, tokenLogProbs, confidence, tokenFrameStarts, tokenFrameEnds)"
        ));
        // Die Dauer muss VOR der Ausgabe stehen, sonst ist das Frame-Ende dort
        // nicht bekannt.
        let duration_at = patched.find("let advance = max(duration, 1)").unwrap();
        let append_at = patched.find("tokenFrameEnds.append").unwrap();
        assert!(duration_at < append_at);
        // Der Vorschub bleibt unveraendert -- nur einmal, und ueber `advance`.
        assert!(patched.contains("                t += advance\n"));
        assert!(!patched.contains("t += max(duration, 1)"));
    }

    #[test]
    fn asr_carries_frames_through_windows_with_offset() {
        let patched = patch_parakeet_asr_source(ASR).unwrap();

        assert!(
            patched.contains(
                "public private(set) var lastWordTimings: [ParakeetVocabulary.WordTiming]?"
            )
        );
        assert!(patched.contains("let frameOffset = start / config.subsamplingFactor"));
        assert!(
            patched.contains("tokenFrameStarts += r.tokenFrameStarts.map { $0 + frameOffset }")
        );
        assert!(patched.contains("lastWordTimings = vocabulary.decodeWordTimings("));
        // Ohne die Log-Wahrscheinlichkeiten kann das Woerterbuch keine
        // Sicherheit je Wort rechnen -- der Parameter ist der ganze Punkt.
        assert!(patched.contains("            logProbs: tokenLogProbs,\n"));
        assert!(patched.contains(
            "secondsPerFrame: Float(config.hopLength * config.subsamplingFactor)\n                / Float(config.sampleRate))"
        ));
        // Das einzelne Fenster bekommt KEINEN Versatz -- es beginnt bei null.
        assert!(patched.contains(
            "            tokenFrameStarts = r.tokenFrameStarts; tokenFrameEnds = r.tokenFrameEnds\n"
        ));
    }

    #[test]
    fn vocabulary_gains_word_timings() {
        let patched = patch_parakeet_vocabulary_source(VOCABULARY).unwrap();

        assert!(patched.contains("public func decodeWordTimings("));
        assert!(patched.contains("guard tokenIds.count == frameStarts.count"));
        // Die Sicherheit je Wort reist im selben Typ wie die Zeit. Eine
        // zweite, parallel gefuehrte Liste waere die Bruchstelle.
        assert!(patched.contains("public struct WordTiming: Sendable {"));
        assert!(patched.contains("        public let confidence: Float\n"));
        assert!(patched.contains("            tokenIds.count == logProbs.count\n"));
        // Dieselbe Rechnung wie `decodeWords` -- sonst traegt dasselbe Wort in
        // zwei Listen zwei verschiedene Sicherheiten.
        assert!(patched.contains("confidence: min(1.0, exp(meanLP))"));
        // Die vorhandene Methode bleibt unangetastet.
        assert!(patched.contains(
            "public func decodeWords(_ tokenIds: [Int], logProbs: [Float]) -> [WordConfidence] {"
        ));
    }

    #[test]
    fn word_frame_patches_are_idempotent() {
        let decoder = patch_tdt_greedy_decoder_source(DECODER).unwrap();
        assert_eq!(patch_tdt_greedy_decoder_source(&decoder).unwrap(), decoder);

        let asr = patch_parakeet_asr_source(ASR).unwrap();
        assert_eq!(patch_parakeet_asr_source(&asr).unwrap(), asr);

        let vocabulary = patch_parakeet_vocabulary_source(VOCABULARY).unwrap();
        assert_eq!(
            patch_parakeet_vocabulary_source(&vocabulary).unwrap(),
            vocabulary
        );
    }

    #[test]
    fn a_second_unpatched_copy_of_an_anchor_is_refused() {
        // Der Fall, den die alte Pruefung durchgelassen haette: eine Stelle ist
        // schon gepatcht, eine zweite -- die tatsaechlich ausgefuehrte -- noch
        // nicht. "Ersetzung kommt irgendwo vor" hiess dort: Schritt erledigt,
        // Bau gelingt, und es werden nie Zeiten gesetzt.
        let patched = patch_parakeet_asr_source(ASR).unwrap();
        let half_patched = format!(
            "{patched}\nextension ParakeetASRModel {{\n    func transcribeAgain() {{\n        lastWordConfidences = vocabulary.decodeWords(tokenIds, logProbs: tokenLogProbs)\n    }}\n}}\n"
        );

        assert!(patch_parakeet_asr_source(&half_patched).is_err());
    }

    #[test]
    fn a_doubly_patched_source_is_refused() {
        // Zwei Ersetzungen sind genauso wenig eindeutig wie eine halbe.
        let patched = patch_tdt_greedy_decoder_source(DECODER).unwrap();
        let doubled = format!("{patched}{patched}");

        assert!(patch_tdt_greedy_decoder_source(&doubled).is_err());
    }

    /// Die Eigenschaft, wie sie eine AELTERE Fassung dieses Patches schrieb
    /// (Stand `26d82f871f`, vor dem Wortsicherheits-Umbau): derselbe Patch,
    /// anderer Typ.
    const OLD_FORK_PROPERTY: &str = "    /// Per-word confidence scores from the last transcription.\n    public private(set) var lastWordConfidences: [WordConfidence]?\n    /// Fork: Wortzeiten der letzten Transkription, aus den Encoder-Frames des\n    /// Dekoders. `nil`, wenn die Frame-Liste nicht zur Token-Liste passt.\n    public private(set) var lastWordTimings: [AlignedWord]?\n";

    #[test]
    fn an_older_fork_patch_is_refused_instead_of_patched_a_second_time() {
        // Der gemessene Fall. Mehrere Ersetzungen beginnen woertlich mit ihrem
        // eigenen Anker -- `PATCHED_PROPERTY` mit `PROPERTY`. In einer Datei
        // mit einer AELTEREN Fassung unseres Patches zaehlt die heutige
        // Ersetzung deshalb null Treffer, waehrend der Anker noch dasteht:
        // genau das Bild `(0, 1)`, das "unberuehrt" heisst. Ohne den
        // Fassungs-Waechter traegt die Datei danach ZWEI `lastWordTimings`.
        let anchor = "    /// Per-word confidence scores from the last transcription.\n    public private(set) var lastWordConfidences: [WordConfidence]?\n";
        let old = ASR.replace(anchor, OLD_FORK_PROPERTY);
        assert_ne!(old, ASR, "die Vorlage muss den Anker enthalten");

        let error = patch_parakeet_asr_source(&old)
            .expect_err("eine fremde Patch-Fassung darf nicht ueberpatcht werden");
        assert_eq!(error, FOREIGN_PATCH_VERSION);
    }

    #[test]
    fn no_fingerprint_appears_in_an_untouched_source() {
        // Die Voraussetzung des Waechters. Stuende ein Fingerabdruck schon in
        // der unberuehrten Quelle, braeche jeder ERSTE Bau ab.
        assert!(!DECODER.contains("tokenFrameStarts"));
        assert!(!ASR.contains("lastWordTimings"));
        assert!(!VOCABULARY.contains("Fork:"));

        // Und er muss nach dem Patchen dastehen, sonst prueft der Waechter auf
        // etwas, das nie entsteht.
        assert!(
            patch_tdt_greedy_decoder_source(DECODER)
                .unwrap()
                .contains("tokenFrameStarts")
        );
        assert!(
            patch_parakeet_asr_source(ASR)
                .unwrap()
                .contains("lastWordTimings")
        );
        assert!(
            patch_parakeet_vocabulary_source(VOCABULARY)
                .unwrap()
                .contains("Fork:")
        );
    }

    #[test]
    fn an_older_fork_patch_is_caught_in_whichever_step_changed() {
        // Der Fall, den eine fruehere Fassung dieses Waechters durchgelassen
        // haette: sie prueft je Datei EINEN Marker aus EINEM der sechs
        // Schritte (die Eigenschaft). Aendert eine kuenftige Fassung nur einen
        // ANDEREN Schritt, steht der geprueftte Marker unveraendert da -- und
        // `PATCHED_WORDS` beginnt woertlich mit `WORDS`, ergibt also wieder das
        // Bild `(0, 1)` = "unberuehrt".
        //
        // Hier ist die Eigenschaft die HEUTIGE und nur der Aufruf eine aeltere
        // Fassung. Genau die Kreuzung, die ein einzelner Marker nicht sieht.
        let today = patch_parakeet_asr_source(ASR).unwrap();
        let older = today.replace("            logProbs: tokenLogProbs,\n", "");
        assert_ne!(
            older, today,
            "die Vorlage muss den heutigen Aufruf enthalten"
        );

        let error = patch_parakeet_asr_source(&older)
            .expect_err("eine fremde Patch-Fassung darf nicht ueberpatcht werden");
        assert_eq!(error, FOREIGN_PATCH_VERSION);
    }

    #[test]
    fn an_already_patched_source_stays_untouched() {
        // Der Alltagsfall: ein warmer Checkout mit der heutigen Fassung wird
        // nicht angefasst. Ohne diese Zusicherung waere der Fassungs-Waechter
        // eine Bremse, die bei jedem zweiten Bau ausloest.
        for (source, patch) in [
            (
                patch_tdt_greedy_decoder_source(DECODER).unwrap(),
                patch_tdt_greedy_decoder_source as fn(&str) -> Result<String, &'static str>,
            ),
            (
                patch_parakeet_asr_source(ASR).unwrap(),
                patch_parakeet_asr_source,
            ),
            (
                patch_parakeet_vocabulary_source(VOCABULARY).unwrap(),
                patch_parakeet_vocabulary_source,
            ),
        ] {
            assert_eq!(patch(&source).unwrap(), source);
        }
    }

    #[test]
    fn a_foreign_method_of_the_same_name_does_not_count_as_patched() {
        // Frueher genuegte der blosse Name `decodeWordTimings` als Beleg. Eine
        // fremde Methode gleichen Namens haette den Patch verhindert, ohne dass
        // irgendetwas gebrochen waere.
        let impostor = VOCABULARY.replace(
            "    /// Decode token IDs into words with per-word confidence scores.\n",
            "    public func decodeWordTimings(_ ids: [Int]) -> [AlignedWord] { return [] }\n\n    /// Decode token IDs into words with per-word confidence scores.\n",
        );

        let patched = patch_parakeet_vocabulary_source(&impostor).unwrap();
        assert!(patched.contains("frameStarts: [Int],"));
    }

    #[test]
    fn word_frame_patches_reject_unknown_sources() {
        assert_eq!(
            patch_tdt_greedy_decoder_source("struct Unrelated {}").unwrap_err(),
            "TDTGreedyDecoder decode signature not found"
        );
        assert_eq!(
            patch_parakeet_asr_source("class Unrelated {}").unwrap_err(),
            "ParakeetASR lastWordConfidences property not found"
        );
        assert_eq!(
            patch_parakeet_vocabulary_source("struct Unrelated {}").unwrap_err(),
            "ParakeetVocabulary decodeWords documentation anchor not found"
        );
    }

    #[test]
    fn decoder_patch_reports_the_block_that_moved() {
        let without_emit = DECODER.replace("tokens.append(tokenId)", "tokens.append(tokenId + 0)");

        assert_eq!(
            patch_tdt_greedy_decoder_source(&without_emit).unwrap_err(),
            "TDTGreedyDecoder token emission block not found"
        );
    }
}
