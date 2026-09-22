//! Fork (03.09.2026): Woerterbuch-Begriffe in die Silben des Modells zerlegen.
//!
//! Wofuer: der spaetere Schubs im Dekoder (Biasing) arbeitet nicht auf Text,
//! sondern auf Token-IDs. `Sedacz` muss also VOR dem Erkennen in die Folge
//! zerfallen, die der Erkenner selbst ausgeben wuerde -- sonst schiebt man
//! Wahrscheinlichkeit auf Tokens, die im Vokabular gar nicht vorkommen.
//!
//! ## Warum die Bibliothek von Hugging Face und keine eigene Zerlegung
//!
//! Gemessen, nicht vermutet. `tokenizer.json` von
//! `nvidia/parakeet-tdt-0.6b-v3` traegt:
//!
//! - `model.type: "BPE"` mit **13 476 Merge-Paaren** (kein Unigram-Modell,
//!   wie man bei "SentencePiece" zuerst annimmt),
//! - `byte_fallback: true` und `fuse_unk: true`,
//! - einen `Normalizer` vom Typ **`Precompiled`** -- ein SentencePiece-
//!   Zeichenabbildungs-Blob von 316 748 Base64-Zeichen, technisch ein
//!   kompilierter Trie.
//!
//! Der letzte Punkt entscheidet. Eine eigene BPE-Schleife waere ueberschaubar;
//! den `Precompiled`-Normalisierer nachzubauen heisst, ein fremdes Binaerformat
//! nachzuimplementieren, dessen Fehler nicht knallen, sondern **stillschweigend
//! andere IDs** liefern. Genau die Klasse Fehler, die man erst am schlechten
//! Erkennungsergebnis merkt.
//!
//! Der Preis dafuer ist gemessen: `cargo add --dry-run` in einem leeren Projekt
//! und gegen das Lockfile dieses Baums abgeglichen ergibt **18 neue Crates** mit
//! Vorgabe-Merkmalen und **14** ohne (es fallen `onig`, `onig_sys` --
//! C-Bindings an Oniguruma --, `indicatif` und `unit-prefix` weg). Deshalb
//! `default-features = false`: wir brauchen weder einen Fortschrittsbalken noch
//! die C-Regex, unser Vorzerleger ist `Metaspace`.
//!
//! Dabei ein gemessener Stolperer: die blosse Abschaltung der Vorgabe-Merkmale
//! uebersetzt NICHT. `tokenizers` verlangt entweder `onig` oder `fancy-regex`
//! (`unresolved import crate::utils::SysRegex`) -- wir nehmen `fancy-regex`,
//! die reine Rust-Fassung. Die Merkmalstabelle auf crates.io sagt das nicht.
//!
//! ## Zur Laufzeit
//!
//! Kein Netz, kein Python. Die Datei liegt neben dem Modell und wird beim Laden
//! gegen Groesse und Pruefsumme gehalten.

use std::path::{Path, PathBuf};

/// Dateiname neben dem Modell.
pub const TOKENIZER_FILE_NAME: &str = "tokenizer.json";

/// Bezugsquelle, nach dem Muster aus `crates/local-model` (ZICK-263): die
/// Adresse des Herstellers selbst, nicht der Speicher eines fremden Projekts.
///
/// Am 03.09.2026 zweimal unabhaengig geladen, beide Male bitgleich (HTTP 200,
/// Revision `541d1f99c6b0c3cd0b11a95167540bb8edefd82b`).
///
/// Die Adresse nennt genau diese Revision und nicht `main`. Sonst zeigt sie auf
/// einen beweglichen Zweig: tauscht NVIDIA die Datei, scheitert ein voellig
/// legitimer Neubezug an der Pruefsumme unten -- und der Fehler liest sich wie
/// eine beschaedigte Datei, obwohl der Hersteller schlicht etwas veroeffentlicht
/// hat. Adresse und Pruefsumme muessen dieselbe Datei meinen.
pub const TOKENIZER_URL: &str = "https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3/resolve/\
     541d1f99c6b0c3cd0b11a95167540bb8edefd82b/tokenizer.json";

/// Groesse der geladenen Datei in Bytes.
pub const TOKENIZER_BYTES: u64 = 1_159_960;

/// CRC32 der vollstaendig geladenen Datei -- dieselbe Rechnung wie
/// `anlg_file::calculate_file_checksum` (crc32fast). **Wird nie nachgezogen**:
/// sie ist der Beweis, dass die Datei dieselbe ist. Passt sie nicht mehr, hat
/// der Hersteller die Datei getauscht, und das ist eine Entscheidung, keine
/// Anpassung.
pub const TOKENIZER_CRC32: u32 = 1_601_548_114;

/// Groesse des KERN-Vokabulars (`model.vocab`), also der gueltige ID-Bereich
/// `0..8191`.
///
/// Am 03.09.2026 an der gepinnten Datei gezaehlt, nicht angenommen:
///
/// - `model.vocab` hat **8192** Eintraege.
/// - `added_tokens` hat **275** Eintraege mit IDs `0..8192`. Davon liegen
///   **274 mitten im Kernvokabular** und stehen dort bei derselben ID auch in
///   `model.vocab`; nur `<blank>` (8192) liegt ausserhalb. Der aeltere
///   Kommentar hier las sich, als sei `<blank>` der einzige Zusatztoken --
///   das stimmt fuer den ID-Bereich, nicht fuer die Menge.
///
/// Diese Zahl faengt also genau einen Fall: `<blank>`. Alle uebrigen
/// Steuertokens (`<unk>`=0, `<pad>`=2, `<|startoftranscript|>`=4, die 30
/// `<|spltoken*|>`, die Sprachmarken) wuerden hier anstandslos durchlaufen --
/// dafuer gibt es [`is_control_piece`].
pub const VOCAB_SIZE: u32 = 8192;

/// Ist dieses Stueck ein Steuertoken?
///
/// Gemessen an der gepinnten `tokenizer.json`: **264** der 8192 Kern-Stuecke
/// haben die Form `<...>`, und sie liegen zusammenhaengend auf `0..233` und
/// `244..273`. Die Luecke `234..243` sind die Ziffern `0`..`9` -- sie stehen
/// zwar ebenfalls in `added_tokens`, sind aber ganz gewoehnliche Stuecke.
///
/// Genau deshalb wird nach der FORM geprueft und nicht nach der Mitgliedschaft
/// in `added_tokens`: die haette die zehn Ziffern miterschlagen, und ein
/// Begriff wie `NOR 24` waere nicht mehr zerlegbar gewesen.
///
/// Die Form ist trennscharf: **kein einziges** gewoehnliches Stueck des
/// Vokabulars enthaelt ueberhaupt ein `<` oder `>`.
fn is_control_piece(piece: &str) -> bool {
    piece.starts_with('<') && piece.ends_with('>') && piece.len() >= 2
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Wortlisten-Zerleger: {path} nicht lesbar: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Die Datei liegt nicht neben dem Modell.
    ///
    /// Eigener Fall und nicht [`Error::Read`], weil er einen anderen Ausgang
    /// hat: hier ist nichts kaputt, es fehlt nur etwas, und der Fehlertext
    /// kann den Weg dorthin gleich mitliefern. Bis zum 03.09.2026 war das ein
    /// stiller Ausfall -- `tokenizer.json` wurde von Hand neben das Modell
    /// gelegt und haengt an keinem Bezug; eine Raeumung des
    /// Hugging-Face-Zwischenspeichers nimmt sie kommentarlos mit.
    #[error(
        "Wortlisten-Zerleger: {path} fehlt. Die Datei haengt (Stand 03.09.2026) \
         an keinem automatischen Bezug und muss von Hand geholt werden:\n\
         curl -L -o {path} {TOKENIZER_URL}"
    )]
    Missing { path: PathBuf },
    #[error(
        "Wortlisten-Zerleger: {path} ist nicht die erwartete Datei \
         ({actual_bytes} Bytes / CRC32 {actual_crc32}, erwartet \
         {TOKENIZER_BYTES} / {TOKENIZER_CRC32}). Quelle: {TOKENIZER_URL}"
    )]
    Checksum {
        path: PathBuf,
        actual_bytes: u64,
        actual_crc32: u32,
    },
    #[error("Wortlisten-Zerleger: {path} laesst sich nicht laden: {message}")]
    Load { path: PathBuf, message: String },
    #[error("Wortlisten-Zerleger: \"{term}\" laesst sich nicht zerlegen: {message}")]
    Encode { term: String, message: String },
    /// Ein Begriff faellt in gar kein Token. Das darf nicht durchrutschen: eine
    /// leere Folge waere ein Schubs auf nichts.
    #[error("Wortlisten-Zerleger: \"{term}\" zerfaellt in kein einziges Token")]
    Empty { term: String },
    /// Eine ID ausserhalb des Kernvokabulars. Der Dekoder wuerde sie nie
    /// ausgeben; ein Schubs darauf ginge ins Leere -- oder schlimmer, auf den
    /// Blank-Token.
    #[error(
        "Wortlisten-Zerleger: \"{term}\" ergibt Token {id}, ausserhalb des \
         Vokabulars 0..{max}"
    )]
    OutOfVocabulary { term: String, id: u32, max: u32 },
    /// Ein Steuertoken mitten im gueltigen ID-Bereich. Er kommt in echtem Text
    /// nicht vor; ein Schubs darauf schoebe Wahrscheinlichkeit auf die
    /// Ablaufsteuerung des Dekoders statt auf eine Silbe.
    #[error(
        "Wortlisten-Zerleger: \"{term}\" ergibt Token {id} ({piece}), einen \
         Steuertoken und keine Silbe"
    )]
    ControlToken {
        term: String,
        id: u32,
        piece: String,
    },
    #[error(
        "Wortlisten-Zerleger: {file} und das Modellvokabular stimmen nicht \
         ueberein: {detail}"
    )]
    VocabularyMismatch { file: PathBuf, detail: String },
}

/// Der Pfad, an dem die Datei neben dem Modell liegt.
pub fn tokenizer_path_next_to_model(model_dir: impl AsRef<Path>) -> PathBuf {
    model_dir.as_ref().join(TOKENIZER_FILE_NAME)
}

/// Groesse und Pruefsumme einer Datei -- getrennt von [`TermTokenizer`], damit
/// ein Downloader sie pruefen kann, ohne den Zerleger zu bauen.
pub fn verify_tokenizer_file(path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;

    let actual_bytes = bytes.len() as u64;
    let actual_crc32 = crc32fast::hash(&bytes);
    if actual_bytes != TOKENIZER_BYTES || actual_crc32 != TOKENIZER_CRC32 {
        return Err(Error::Checksum {
            path: path.to_path_buf(),
            actual_bytes,
            actual_crc32,
        });
    }

    Ok(())
}

/// Die Zusicherungen ueber eine fertige ID-Folge -- als freie Funktion, damit
/// sie pruefbar sind, ohne einen Begriff zu finden, der sie ausloest.
///
/// Die Nachschlage-Funktion kommt von aussen herein statt ueber `&self`: so
/// laesst sich der Steuertoken-Fall mit einem erfundenen Vokabular pruefen,
/// ohne das echte Modell zu brauchen. Ein Waechter, den kein Test erreicht, ist
/// eine Behauptung.
///
/// Drei Faelle, in dieser Reihenfolge:
///
/// 1. leere Folge -- ein Schubs auf nichts;
/// 2. ID jenseits des Kernvokabulars -- heute nur `<blank>` (8192);
/// 3. ein Steuertoken MITTEN im gueltigen Bereich. Das war die Luecke bis zum
///    03.09.2026: Fall 2 faengt genau eine der 264 Steuermarken, weil nur sie
///    ausserhalb liegt. Erreichbar ist der Fall, wenn jemand `<|de|>` oder
///    `<pad>` woertlich in seine Wortliste schreibt -- die Bibliothek erkennt
///    solche Zeichenketten als Zusatztoken, unabhaengig von
///    `add_special_tokens: false`.
fn guard_ids(
    term: &str,
    ids: Vec<u32>,
    piece: impl Fn(u32) -> Option<String>,
) -> Result<Vec<u32>, Error> {
    if ids.is_empty() {
        return Err(Error::Empty {
            term: term.to_string(),
        });
    }
    if let Some(&id) = ids.iter().find(|&&id| id >= VOCAB_SIZE) {
        return Err(Error::OutOfVocabulary {
            term: term.to_string(),
            id,
            max: VOCAB_SIZE - 1,
        });
    }
    for &id in &ids {
        if let Some(text) = piece(id)
            && is_control_piece(&text)
        {
            return Err(Error::ControlToken {
                term: term.to_string(),
                id,
                piece: text,
            });
        }
    }

    Ok(ids)
}

/// Zerlegt Begriffe in die Token-IDs des Parakeet-Vokabulars.
pub struct TermTokenizer {
    inner: tokenizers::Tokenizer,
}

impl TermTokenizer {
    /// Laden UND gegen das Modell halten, das danebenliegt.
    ///
    /// [`Self::check_against_model_vocabulary`] stand bis zum 03.09.2026
    /// daneben statt im Weg: kein Aufrufer rief ihn ausser den Tests. Ein
    /// Waechter, den niemand ruft, schuetzt nichts -- die Zerlegung koennte
    /// sauber aussehen und trotzdem auf ein anderes Vokabular zeigen als der
    /// Erkenner, und der Schubs ginge auf fremde Silben, ohne dass irgendwo
    /// etwas bricht.
    ///
    /// **Ehrlich zum Stand:** dieses ganze Modul hat heute UEBERHAUPT keinen
    /// Produktivaufrufer -- der Schubs im Dekoder, fuer den die Zerlegung
    /// gedacht ist, ist noch nicht gebaut. Die Luecke ist damit VORBEREITET
    /// geschlossen, nicht geschlossen: wer den Zerleger neben einem Modell
    /// braucht, findet hier den Weg, auf dem er die Pruefung nicht vergessen
    /// kann. [`Self::from_verified_file`] bleibt fuer den Fall ohne Modell
    /// daneben; sobald der echte Aufrufer da ist, gehoert er auf `pub(crate)`
    /// heruntergestuft, damit es keinen zweiten Weg mehr gibt.
    pub fn load_next_to_model(model_dir: impl AsRef<Path>) -> Result<Self, Error> {
        let model_dir = model_dir.as_ref();
        let path = tokenizer_path_next_to_model(model_dir);
        if !path.is_file() {
            return Err(Error::Missing { path });
        }

        let tokenizer = Self::from_verified_file(&path)?;

        let vocab_path = model_dir.join("vocab.json");
        let vocab_json = std::fs::read_to_string(&vocab_path).map_err(|source| Error::Read {
            path: vocab_path.clone(),
            source,
        })?;
        tokenizer.check_against_model_vocabulary(&vocab_json, &vocab_path)?;

        Ok(tokenizer)
    }

    /// Laedt die Datei NACH der Pruefung von Groesse und Pruefsumme.
    pub fn from_verified_file(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        verify_tokenizer_file(path)?;
        Self::from_file_unchecked(path)
    }

    /// Ohne Pruefsummen-Pruefung -- fuer Tests mit erfundenen Vokabularen.
    pub fn from_file_unchecked(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let inner = tokenizers::Tokenizer::from_file(path).map_err(|error| Error::Load {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;

        Ok(Self { inner })
    }

    /// Ein Begriff, eine ID-Folge.
    ///
    /// `add_special_tokens: false` -- gezerlegt wird der Begriff, nicht ein
    /// Satz. Ein `<|de|>`-Sprachtoken davor waere im Schubs schlicht falsch.
    pub fn split_term(&self, term: &str) -> Result<Vec<u32>, Error> {
        let encoding = self
            .inner
            .encode(term, false)
            .map_err(|error| Error::Encode {
                term: term.to_string(),
                message: error.to_string(),
            })?;

        guard_ids(term, encoding.get_ids().to_vec(), |id| self.piece(id))
    }

    /// Eine ganze Wortliste. Bricht beim ersten Begriff ab, der nicht traegt --
    /// eine halb zerlegte Liste waere ein Schubs, von dem niemand weiss, worauf.
    pub fn split_terms<S: AsRef<str>>(&self, terms: &[S]) -> Result<Vec<Vec<u32>>, Error> {
        terms
            .iter()
            .map(|term| self.split_term(term.as_ref()))
            .collect()
    }

    /// Das Stueck zu einer ID -- der Rueckweg fuer den Rundlauf-Beweis.
    pub fn piece(&self, id: u32) -> Option<String> {
        self.inner.id_to_token(id)
    }

    /// Setzt die Stuecke einer ID-Folge zurueck zum Text zusammen.
    ///
    /// Von Hand und nicht ueber `decode`: der Rundlauf soll zeigen, dass die
    /// STUECKE den Begriff ergeben, nicht dass die Bibliothek ihren eigenen
    /// Dekodierer findet. `▁` ist die SentencePiece-Wortgrenze.
    pub fn join_pieces(&self, ids: &[u32]) -> Option<String> {
        let mut out = String::new();
        for &id in ids {
            let piece = self.piece(id)?;
            out.push_str(&piece.replace('\u{2581}', " "));
        }
        Some(out.trim().to_string())
    }

    /// Groesse des Kernvokabulars, OHNE Zusatztoken.
    pub fn core_vocab_size(&self) -> usize {
        self.inner.get_vocab_size(false)
    }

    /// Waechter: traegt diese Datei dasselbe Vokabular wie das Modell?
    ///
    /// Die Zerlegung ist nur dann etwas wert, wenn ID 681 hier und im Erkenner
    /// dasselbe Stueck meint. Verglichen wird gegen die `vocab.json`, die neben
    /// dem CoreML-Modell liegt (ID-String -> Stueck) -- also gegen genau das
    /// Vokabular, das `ParakeetVocabulary` in speech-swift laedt.
    pub fn check_against_model_vocabulary(
        &self,
        vocab_json: &str,
        file: impl AsRef<Path>,
    ) -> Result<(), Error> {
        let file = file.as_ref().to_path_buf();
        let map: std::collections::BTreeMap<String, String> = serde_json::from_str(vocab_json)
            .map_err(|error| Error::VocabularyMismatch {
                file: file.clone(),
                detail: format!("vocab.json ist nicht lesbar: {error}"),
            })?;

        if map.len() != self.core_vocab_size() {
            return Err(Error::VocabularyMismatch {
                file,
                detail: format!(
                    "Groesse: vocab.json hat {}, tokenizer.json {}",
                    map.len(),
                    self.core_vocab_size()
                ),
            });
        }

        for (raw_id, piece) in &map {
            let id: u32 = raw_id.parse().map_err(|_| Error::VocabularyMismatch {
                file: file.clone(),
                detail: format!("vocab.json traegt den Schluessel {raw_id:?}, keine Zahl"),
            })?;
            match self.piece(id) {
                Some(ours) if &ours == piece => {}
                other => {
                    return Err(Error::VocabularyMismatch {
                        file,
                        detail: format!(
                            "ID {id}: Modell sagt {piece:?}, tokenizer.json sagt {other:?}"
                        ),
                    });
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
