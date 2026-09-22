use super::*;

fn word(text: &str) -> ScanWord {
    ScanWord {
        text: format!(" {text}"),
        measured_confidence: None,
    }
}

fn session(id: &str, text: &str, names: &[&str]) -> ScanSession {
    ScanSession {
        session_id: id.to_string(),
        words: text.split(' ').map(word).collect(),
        context_names: names.iter().map(|name| name.to_string()).collect(),
        context_emails: vec![],
        context_text: String::new(),
        generated_title: String::new(),
    }
}

/// Wie `session`, aber die Teilnehmer kommen als MAILADRESSEN herein.
///
/// Ein eigener Bauer, weil genau darin die Zusage steckt: was ueber dieses
/// Feld hereinkommt, darf Immunitaet stiften und nie ein Ziel werden.
fn session_mit_adressen(id: &str, text: &str, adressen: &[&str]) -> ScanSession {
    ScanSession {
        session_id: id.to_string(),
        words: text.split(' ').map(word).collect(),
        context_names: vec![],
        context_emails: adressen.iter().map(|wert| wert.to_string()).collect(),
        context_text: String::new(),
        generated_title: String::new(),
    }
}

/// Wie `session_mit_adressen`, aber alle Woerter tragen eine HOHE gemessene
/// Sicherheit -- damit laeuft ausschliesslich Netz 1.
///
/// Netz 2 steigt bei Woertern ueber `confidence_ceiling` aus (`proposals.rs`,
/// Netz 2), Netz 1 sieht die Sicherheit gar nicht an. Ohne diesen Bauer
/// beantwortet Netz 2 jede Frage zuerst, und ein Test ueber Netz 1 bleibt
/// gruen, obwohl Netz 1 nie gelaufen ist -- am 03.09.2026 genau so passiert,
/// zwei Mutanten ueberlebten es.
fn session_nur_netz_eins(id: &str, text: &str, adressen: &[&str]) -> ScanSession {
    ScanSession {
        words: text
            .split(' ')
            .map(|wort| ScanWord {
                text: format!(" {wort}"),
                measured_confidence: Some(1.0),
            })
            .collect(),
        ..session_mit_adressen(id, text, adressen)
    }
}

/// Wie `session`, aber mit Titel-/Notiz-Kontext statt Teilnehmern.
fn session_mit_text(id: &str, text: &str, context_text: &str) -> ScanSession {
    ScanSession {
        session_id: id.to_string(),
        words: text.split(' ').map(word).collect(),
        context_names: vec![],
        context_emails: vec![],
        context_text: context_text.to_string(),
        generated_title: String::new(),
    }
}

/// Wie `session`, aber mit einem VOM MODELL ERZEUGTEN Titel.
fn session_mit_erzeugtem_titel(id: &str, text: &str, titel: &str) -> ScanSession {
    ScanSession {
        session_id: id.to_string(),
        words: text.split(' ').map(word).collect(),
        context_names: vec![],
        context_emails: vec![],
        context_text: String::new(),
        generated_title: titel.to_string(),
    }
}

fn pairs(proposals: &[Proposal]) -> Vec<(String, String)> {
    proposals
        .iter()
        .map(|proposal| (proposal.canonical.clone(), proposal.alias.clone()))
        .collect()
}

// -------------------------------------------------------------------------
// Netz 1
// -------------------------------------------------------------------------

/// Die Kandidatenstufe, einzeln.
///
/// Der Ende-zu-Ende-Test darunter beweist die WIRKUNG, aber nicht, WELCHE der
/// drei Stufen sie erzeugt -- alle drei wuerden das Paar allein aufhalten. Wer
/// nur ihn hat, kann jede einzelne Stufe entfernen und bleibt gruen. Diese
/// drei kleinen Tests fassen je eine Stufe an.
#[test]
fn ein_alltagswort_ist_kein_kandidat() {
    let sitzung = session("s1", "wir sprechen ueber Medien und Grandpfeil heute", &[]);
    let kerne: Vec<String> = collect_candidates(&sitzung, &Options::default())
        .into_iter()
        .map(|kandidat| kandidat.core)
        .collect();
    assert_eq!(
        kerne,
        vec!["Grandpfeil".to_string()],
        "ein gewoehnliches Inhaltswort darf gar nicht erst Kandidat sein"
    );
}

/// Die Ankerstufe, einzeln.
///
/// Ein Kalendertitel ist FREIER TEXT und enthaelt gewoehnliche deutsche
/// Woerter. Zaehlt jedes davon als belegte Schreibweise, erklaert der Lauf
/// jedes aehnlich klingende Wort zu dessen Verhoerung -- das war die Quelle
/// von "Messen <- Medien".
#[test]
fn ein_alltagswort_im_kalendertext_wird_kein_anker() {
    let felder = context_text_fields("Agenda Medien und Grandpfeil");
    assert_eq!(
        felder.anker,
        vec!["Grandpfeil".to_string()],
        "aus freiem Text darf nur das ankern, was kein Alltagswort ist"
    );
}

/// Die Zielstufe von Netz 2, einzeln.
///
/// Sie ist die einzige der drei, die auch fuer WOERTERBUCH-Eintraege greift --
/// die laufen nicht durch den Kalendertext-Filter. Ein Eintrag, dessen
/// richtige Schreibweise ein Alltagswort ist, wuerde sonst jedes aehnlich
/// klingende Wort einsammeln.
#[test]
fn ein_alltagswort_aus_dem_woerterbuch_wird_kein_ziel() {
    let sitzungen = vec![session("s1", "wir haben Mediun besprochen", &[])];
    let options = Options::default();
    assert!(
        scan(&sitzungen, &["Medien".to_string()], &[], &options).is_empty(),
        "ein Alltagswort im Woerterbuch darf kein Ziel sein"
    );

    let sitzungen = vec![session("s1", "wir haben Grandfall besprochen", &[])];
    assert_eq!(
        pairs(&scan(&sitzungen, &["Grandpfeil".to_string()], &[], &options)),
        vec![("Grandpfeil".to_string(), "Grandfall".to_string())],
        "ein Name im Woerterbuch bleibt ein Ziel"
    );
}

/// Der Befund des Betreibers vom 04.09.2026, als Test.
///
/// Sein Satz dazu: "Ich kann doch nicht das Wort Medien durch das Wort messen
/// tauschen." Genau das hat der Lauf vorgeschlagen -- die beiden klingen
/// gleich, und die Handliste kannte keines von beiden, also galten beide als
/// verdaechtig.
///
/// Die Gegenprobe im selben Test ist die Haelfte, die zaehlt: derselbe Aufbau
/// mit erfundenen NAMEN liefert weiterhin einen Vorschlag. Wer die grosse
/// Wortliste eines Tages so weit spannt, dass sie auch Namen erfasst, wird an
/// der zweiten Haelfte rot -- und nicht erst in des Betreibers Postfach.
#[test]
fn zwei_gewoehnliche_inhaltswoerter_ergeben_keinen_vorschlag() {
    let gewoehnlich = vec![
        session_mit_text("s1", "wir sprechen ueber Messen heute", "Agenda Medien"),
        session_mit_text("s2", "und dann ueber Medien sowieso", ""),
    ];
    assert!(
        scan(&gewoehnlich, &[], &[], &Options::default()).is_empty(),
        "zwei Alltagswoerter duerfen kein Paar ergeben: {:?}",
        pairs(&scan(&gewoehnlich, &[], &[], &Options::default()))
    );

    let namen = vec![
        session_mit_text("s1", "wir sprechen ueber Grandfall heute", "Agenda Grandpfeil"),
        session_mit_text("s2", "und dann ueber Grandpfeil sowieso", ""),
    ];
    assert_eq!(
        pairs(&scan(&namen, &[], &[], &Options::default())),
        vec![("Grandpfeil".to_string(), "Grandfall".to_string())],
        "derselbe Aufbau mit Namen muss weiterhin einen Vorschlag liefern"
    );
}

#[test]
fn zwei_schreibweisen_aus_zwei_gespraechen_werden_vorgeschlagen() {
    let sessions = [
        session("a", "wir haben Grandpfeil gefragt und", &[]),
        session("b", "dann hat Grandfall geantwortet und", &[]),
    ];
    let found = scan(&sessions, &["Grandpfeil".into()], &[], &Options::default());
    assert!(
        pairs(&found).contains(&("Grandpfeil".into(), "Grandfall".into())),
        "erwartet Grandpfeil <- Grandfall, bekommen: {:?}",
        pairs(&found)
    );
}

/// Der Falsifikator aus dem Auftrag: ein ueberall gleich geschriebener
/// Begriff erzeugt keinen Vorschlag.
#[test]
fn ein_ueberall_gleich_geschriebener_begriff_erzeugt_keinen_vorschlag() {
    let sessions = [
        session("a", "wir haben Grandpfeil gefragt und", &[]),
        session("b", "dann hat Grandpfeil geantwortet und", &[]),
        session("c", "spaeter kam Grandpfeil nochmal dazu", &[]),
    ];
    let found = scan(&sessions, &["Grandpfeil".into()], &[], &Options::default());
    assert!(found.is_empty(), "unerwartet: {:?}", pairs(&found));
}

/// Das Signal von Netz 1 ist nur im Stapel sichtbar. Beide Schreibweisen in
/// EINEM Gespraech sind eine Beobachtung, kein Muster.
///
/// Ohne Woerterbuch-Eintrag, damit hier wirklich Netz 1 geprueft wird -- mit
/// Eintrag greift Netz 2 und findet dasselbe Paar zu Recht auch im
/// Einzelgespraech.
#[test]
fn zwei_schreibweisen_in_nur_einem_gespraech_reichen_netz_eins_nicht() {
    // Der Anker steht bei einem ANDEREN Gespraech (Teilnehmer von "a"), und
    // "b" hat keinen eigenen Kontext. Damit hat Netz 2 in "b" ueberhaupt kein
    // Ziel und kann den Fund nicht machen -- was hier zaehlt, ist allein die
    // Gespraechszahl von Netz 1.
    //
    // Zwei fruehere Anlaeufe bestanden aus dem falschen Grund: ohne Anker
    // fehlte die belegte Schreibweise, und mit einem Woerterbuch-Anker fand
    // Netz 2 dasselbe Paar zu Recht schon im Einzelgespraech. Beide
    // ueberlebten den Mutanten, gegen den der Test geschrieben ist.
    let anker = session("a", "hallo zusammen wie geht es euch", &["Meiners"]);

    let ein_gespraech = [
        anker.clone(),
        session(
            "b",
            "wir haben Meiners gefragt und dann Meinerz gehoert",
            &[],
        ),
    ];
    let found = scan(&ein_gespraech, &[], &[], &Options::default());
    assert!(found.is_empty(), "unerwartet: {:?}", pairs(&found));

    // Gegenprobe: dieselben zwei Schreibweisen, auf zwei Gespraeche verteilt.
    // Das ist die EINZIGE Aenderung -- und jetzt kommt der Vorschlag.
    let zwei_gespraeche = [
        anker,
        session("b", "wir haben Meiners gefragt und", &[]),
        session("c", "dann hat Meinerz geantwortet und", &[]),
    ];
    assert_eq!(
        pairs(&scan(&zwei_gespraeche, &[], &[], &Options::default())),
        vec![("Meiners".into(), "Meinerz".into())]
    );
}

/// Die Mehrheit darf die Wahrheit nicht bestimmen.
///
/// Gemessen an des Betreibers Korpus: "Grandfall" steht haeufiger da als
/// "Grandpfeil". Waere die Haeufigkeit allein entscheidend, schluege das Netz
/// vor, kuenftig "Grandpfeil" durch "Grandfall" zu ersetzen -- und der
/// Fehlgriff wirkte in jedem kuenftigen Gespraech.
#[test]
fn der_woerterbuch_eintrag_schlaegt_die_haeufigkeit() {
    let sessions = [
        session("a", "erst kam Grandfall dann kam Grandfall wieder", &[]),
        session("b", "hier stand Grandfall und dort Grandpfeil auch", &[]),
    ];
    let found = scan(&sessions, &["Grandpfeil".into()], &[], &Options::default());
    assert_eq!(
        pairs(&found),
        vec![("Grandpfeil".into(), "Grandfall".into())]
    );
}

/// Ohne Beleg wird nicht vorgeschlagen -- auch nicht nach Mehrheit.
///
/// GEMESSEN am 03.09.2026 an des Betreibers Korpus: mit einem Mehrheits-Rueckfall
/// lieferte Netz 1 242 Vorschlaege, davon 240 ohne Anker -- und unter diesen
/// 240 war kein einziger fehlender Woerterbuch-Eintrag. Nur Beugungen
/// ("Aufgabe"/"Aufgaben") und verwandte Alltagswoerter ("Sorge"/"Source",
/// "Kachel"/"Kamera"). Zwei unbekannte Schreibweisen sagen nicht, welche die
/// richtige ist; wer da die haeufigere nimmt, raet mit Statistik.
#[test]
fn ohne_belegte_schreibweise_gibt_es_keinen_vorschlag() {
    let sessions = [
        session("a", "hier stand Meiners und nochmal Meiners dazu", &[]),
        session("b", "dort stand Meinerz einmal und sonst nichts", &[]),
    ];
    let found = scan(&sessions, &[], &[], &Options::default());
    assert!(
        found.is_empty(),
        "geraten statt belegt: {:?}",
        pairs(&found)
    );
}

/// Der Titel oder die Kalenderbeschreibung belegt die Schreibweise genauso
/// wie ein Woerterbuch-Eintrag.
///
/// Das ist kein Beiwerk: "Tofmann" steht in des Betreibers Korpus in KEINER
/// Teilnehmerliste und in keinem Woerterbuch-Eintrag -- nur in der
/// Kalenderbeschreibung des Kundentermin-Termins. Ohne diese Quelle faellt das
/// Paar "Tofmann"/"Tochmann" aus dem Netz.
#[test]
fn der_kalendertext_belegt_die_richtige_schreibweise() {
    // Der Aufbau isoliert die ANKER-Rolle des Kalendertextes.
    //
    // Die Verhoerung steht in Gespraech "b", das selbst gar keinen Kontext
    // hat -- Netz 2 findet dort nichts, weil es kein Ziel gibt. Bleibt Netz 1,
    // und dessen Anker-Menge gilt korpusweit: die richtige Schreibweise kommt
    // aus dem Kalendertext von "a".
    //
    // Ein frueherer Anlauf legte beides in dasselbe Gespraech; dann fand
    // Netz 2 das Paar ueber seine Ziel-Liste, und der Test sagte ueber die
    // Anker-Rolle nichts aus.
    let sessions = [
        session_mit_text(
            "a",
            "wir haben Tofmann schon gefragt ja",
            "Termin mit Tofmann",
        ),
        session("b", "und dann hat Tochmann uns geantwortet ja", &[]),
    ];
    let found = scan(&sessions, &[], &[], &Options::default());
    assert!(
        pairs(&found).contains(&("Tofmann".into(), "Tochmann".into())),
        "erwartet Tofmann <- Tochmann, bekommen: {:?}",
        pairs(&found)
    );
}

/// Derselbe Kalendertext, die ANDERE Rolle: er liefert Netz 2 sein ZIEL.
///
/// Der Test darueber isoliert bewusst die Anker-Rolle, und die laeuft ueber
/// `context_text_fields` direkt. Die Ziel-Rolle laeuft ueber
/// `context_text_terms`, und die war bis zum 04.09.2026 nur von einem
/// Zerleger-Test gedeckt -- also von einer Pruefung auf die Hilfsfunktion,
/// nicht auf den Weg. Der Mutant, der `context_text_terms` auf die falsche
/// Menge umstellte, starb deshalb an einem Zerleger-Test und nicht an einer
/// Verhaltenszusage.
///
/// Ein einziges Gespraech erreicht die Sitzungsschwelle von Netz 1 nie -- hier
/// laeuft also nur Netz 2.
///
/// WIRD ROT, wenn der Kalendertext Netz 2 kein Ziel mehr liefert.
#[test]
fn der_kalendertext_liefert_netz_zwei_sein_ziel() {
    let sessions = [session_mit_text(
        "a",
        "und dann hat Tochmann uns geantwortet ja",
        "Termin mit Tofmann",
    )];
    let found = scan(&sessions, &[], &[], &Options::default());
    assert!(
        pairs(&found).contains(&("Tofmann".into(), "Tochmann".into())),
        "erwartet Tofmann <- Tochmann aus Netz 2, bekommen: {:?}",
        pairs(&found)
    );
    // Die Quelle mit pruefen: sonst bestuende der Test auch, wenn das Ziel
    // ueber eine ganz andere Quelle hereinkaeme.
    assert!(
        found
            .iter()
            .any(|vorschlag| vorschlag.source == "Titel oder Notiz"),
        "das Ziel kam nicht aus Titel oder Notiz: {:?}",
        found.iter().map(|p| p.source.clone()).collect::<Vec<_>>()
    );
}

/// Zwei Beugungen desselben Wortes sind Grammatik, keine Verhoerung.
///
/// GEMESSEN: der groesste einzelne Rauschblock des ersten Laufs war Singular
/// gegen Plural. Ein Eintrag "Aufgabe => Aufgaben" wuerde in jedem kuenftigen
/// Gespraech die Mehrzahl kaputtmachen.
#[test]
fn eine_beugung_ist_keine_verhoerung() {
    let sessions = [
        session("a", "das sind die Aufgaben die wir haben", &[]),
        session("b", "und das ist die Aufgabe von uns", &[]),
    ];
    let found = scan(&sessions, &["Aufgabe".into()], &[], &Options::default());
    assert!(
        found.is_empty(),
        "Beugung vorgeschlagen: {:?}",
        pairs(&found)
    );

    assert!(is_inflection("Aufgabe", "Aufgaben"));
    // "Verlin"/"Werlin" scheitert am gemeinsamen Anfang von 0 %, nicht an der
    // Laengengrenze -- die Zeile steht hier nur noch als Zustandsbeschreibung.
    // Als Gegenprobe FUER DIE LAENGENGRENZE taugte sie nie, siehe den Test
    // darunter.
    assert!(!is_inflection("Verlin", "Werlin"));
    // Und "Grandpfeil"/"Grandfall" teilen zu wenig Anfang, um eine Beugung
    // zu sein -- der Fall aus dem Auftrag bleibt ausdruecklich im Netz.
    assert!(!is_inflection("Grandpfeil", "Grandfall"));
}

/// Die Laengengrenze der Beugungsregel, an ihrer Grenze geprueft.
///
/// Die alte Gegenprobe war eine Attrappe: `is_inflection("Verlin", "Werlin")`
/// ist falsch, weil die beiden 0 % gemeinsamen Anfang haben -- die Bedingung
/// `shorter < MIN_LEN` wird dabei gar nicht wirksam. Wer `MIN_LEN` gestrichen
/// haette, waere gruen durchgekommen. Das ist dieselbe Klasse, die in der
/// ersten Pruefrunde der teuerste Befund war.
///
/// Diese Paare pruefen die Grenze wirklich: gleicher Anfang, angehaengtes "s",
/// EINZIGER Unterschied ist die Laenge. Vier Zeichen sind zu kurz, fuenf
/// reichen. Streicht jemand `MIN_LEN`, wird die erste Zeile rot.
///
/// Die Grenze ist am 04.09.2026 von sechs auf fuenf gewandert, und der Anlass
/// steht im Korpus: der Genitiv eines fuenfbuchstabigen Vornamens ("von
/// Marens Mail") wurde als Verhoerung dieses Vornamens vorgeschlagen. Bei VIER
/// bleibt es dabei, dass gar nicht geprueft wird -- dort haengen drei belegte
/// Vorschlaege dran, in denen ein vierbuchstabiger Vorname gegen eine echte
/// Verhoerung steht.
#[test]
fn die_laengengrenze_der_beugungsregel_wirkt_wirklich() {
    assert!(
        !is_inflection("Merk", "Merks"),
        "vier Zeichen sind zu kurz fuer die Beugungsregel"
    );
    assert!(
        is_inflection("Meier", "Meiers"),
        "fuenf Zeichen sind seit dem 04.09.2026 genau die Grenze"
    );
    assert!(
        is_inflection("Meiner", "Meiners"),
        "sechs Zeichen waren die alte Grenze und bleiben eine Beugung"
    );
}

/// Ein zusammengesetztes Wort ist keine Verhoerung seines ersten Teils.
///
/// GEMESSEN am 04.09.2026: der Lauf schlug vor, kuenftig ein Kompositum durch
/// sein eigenes Bestimmungswort zu ersetzen -- ein Eintrag daraus haette in
/// jedem kuenftigen Gespraech jedes solche Kompositum zerschlagen.
///
/// Die zweite Zeile ist die Gegenprobe, an der eine BEQUEMERE Fassung dieser
/// Regel gestorben waere: "der Rest ist mindestens drei Zeichen lang" haette
/// hier zugeschlagen und einen belegten Vorschlag getoetet. Der Rest muss
/// selbst ein gewoehnliches Wort sein, und "flix" ist keines -- "gramm" schon.
#[test]
fn ein_kompositum_ist_keine_verhoerung_seines_ersten_teils() {
    assert!(
        is_inflection("Kampagne", "Kampagnegramm"),
        "der angehaengte Wortteil ist selbst ein gewoehnliches Wort"
    );
    assert!(
        !is_inflection("Sarko", "Sarkoflix"),
        "ein Rest, der kein gewoehnliches Wort ist, macht aus dem Paar keine Zusammensetzung"
    );
}

/// Der Abstand hat KEINE Untergrenze -- und das aendert genau ein Fenster.
///
/// Die Begruendung in `Options::default` nannte bis zum 03.09.2026
/// "Verlin"/"Merlin" als das Paar, das die gestrichene Untergrenze
/// zusammengebracht habe. Nachgerechnet stimmt das nicht: beide sind sechs
/// Zeichen lang, `floor(6 * 0,4)` ist 2, und `max(2, 2)` ist ebenfalls 2 --
/// die Untergrenze hat an diesem Paar nie etwas getan. Sie wirkte
/// AUSSCHLIESSLICH auf Woerter mit vier Zeichen, also auf genau die kuerzesten
/// zugelassenen Kandidaten, und verdoppelte dort die Erlaubnis von 1 auf 2.
///
/// Dieser Test steht an dieser Laenge und nirgends sonst.
#[test]
fn bei_vier_zeichen_ist_genau_ein_buchstabe_erlaubt() {
    let optionen = Options::default();
    assert_eq!(max_edits_for("Zerk", "Zirk", &optionen), 1);

    // Ein Buchstabe Unterschied: zusammen.
    assert!(sounds_alike("Zerk", "Zirk", &optionen));
    // Zwei Buchstaben: getrennt -- obwohl der KLANG sie durchlaesst.
    assert!(
        levenshtein(&encode("Zerk"), &encode("Zink")) <= optionen.max_code_edits,
        "Test taugt nicht: schon der Klang trennt die beiden"
    );
    assert!(!sounds_alike("Zerk", "Zink", &optionen));
}

/// Ein belegt richtiges Wort wird NIE als Verhoerung vorgeschlagen.
///
/// Der teuerste Fehlgriff des ersten Laufs: achtzehn Mal schlug das Netz vor,
/// "Verlin" durch den Woerterbuch-Eintrag "Merlin" zu ersetzen -- Klang-Abstand
/// eins, Buchstaben-Abstand zwei. Verlin ist Teilnehmer, sein Name ist belegt
/// richtig. Die Immunitaet gilt ueber den ganzen Korpus: dass Verlin in einem
/// anderen Termin nicht eingeladen war, macht seinen Namen nicht falsch.
#[test]
fn ein_belegter_name_wird_nie_zur_verhoerung() {
    let sessions = [
        session(
            "a",
            "das sagst du so leicht Verlin wie soll das gehen",
            &["Mads Verlin | nordwerk"],
        ),
        // Zweites Gespraech OHNE Verlin in der Teilnehmerliste -- trotzdem
        // bleibt sein Name geschuetzt.
        session("b", "da hat Verlin uns nochmal was geschickt ja", &[]),
    ];
    let found = scan(&sessions, &["Merlin".into()], &[], &Options::default());
    assert!(
        !found.iter().any(|proposal| proposal.alias == "Verlin"),
        "belegter Name als Verhoerung vorgeschlagen: {:?}",
        pairs(&found)
    );
}

/// Gewoehnliches Deutsch bleibt in Ruhe -- derselbe Waechter, den der
/// Nachlauf anlegt.
#[test]
fn gewoehnliche_deutsche_woerter_erzeugen_keinen_vorschlag() {
    let sessions = [
        session("a", "das ist Wieder ein Wieder gewesen ja", &[]),
        session("b", "und das war Weider nochmal so gemeint", &[]),
    ];
    let found = scan(&sessions, &[], &[], &Options::default());
    assert!(
        !pairs(&found)
            .iter()
            .any(|(canonical, alias)| canonical == "Wieder" || alias == "Wieder"),
        "gewoehnliches Deutsch vorgeschlagen: {:?}",
        pairs(&found)
    );
}

/// Ein Satzanfang ist grossgeschrieben, weil er ein Satzanfang ist.
#[test]
fn ein_satzanfang_ist_kein_kandidat() {
    let liste = ["Meiners".to_string()];

    // Mitten im Satz: gefunden.
    let im_satz = [
        session("a", "also hat Meiners uns das gesagt", &[]),
        session("b", "also hat Meinerz uns das gesagt", &[]),
    ];
    assert_eq!(
        pairs(&scan(&im_satz, &liste, &[], &Options::default())),
        vec![("Meiners".into(), "Meinerz".into())]
    );

    // Dieselben Woerter, aber jeweils direkt hinter einem Punkt: kein
    // Vorschlag. Das ist die einzige Aenderung zwischen den beiden Faellen.
    let am_satzanfang = [
        session("a", "also hat er das gesagt. Meiners kam dazu", &[]),
        session("b", "also hat er das gesagt. Meinerz kam dazu", &[]),
    ];
    let found = scan(&am_satzanfang, &liste, &[], &Options::default());
    assert!(found.is_empty(), "unerwartet: {:?}", pairs(&found));
}

/// Ein vom Modell erzeugter Titel belegt GAR NICHTS.
///
/// Der Titel eines Gespraechs ohne Kalendertermin schreibt ein Sprachmodell
/// aus demselben Transkript, das hier geprueft wird. Uebernimmt es dabei eine
/// Verhoerung, belegt der Titel danach sich selbst -- und dreht die Wahrheit
/// um: aus richtig geschriebenem "Grandpfeil" im Transkript wuerde der
/// Vorschlag, es kuenftig durch "Grandfall" zu ersetzen.
#[test]
fn ein_erzeugter_titel_ist_kein_beleg() {
    let sessions = [
        session_mit_erzeugtem_titel(
            "a",
            "wir haben Grandpfeil gefragt und",
            "Grandfall Strategie",
        ),
        session("b", "dann hat Grandfall geantwortet und", &[]),
    ];
    let found = scan(&sessions, &[], &[], &Options::default());
    assert!(
        found.is_empty(),
        "der erzeugte Titel wurde als Beleg genommen: {:?}",
        pairs(&found)
    );

    // Die Gegenprobe: derselbe Text als KALENDER-Beschreibung -- die hat ein
    // Mensch getippt, und sie belegt. Das ist die einzige Aenderung.
    let vom_menschen = [
        session_mit_text(
            "a",
            "wir haben Grandpfeil gefragt und",
            "Grandfall Strategie",
        ),
        session("b", "dann hat Grandfall geantwortet und", &[]),
    ];
    assert!(!scan(&vom_menschen, &[], &[], &Options::default()).is_empty());
}

/// Ein Doppelname schuetzt auch seine Haelften.
///
/// "Mads-Peter Verlin" legte bis zum 03.09.2026 nur "mads-peter" und "verlin"
/// in die Anker-Menge. Im Transkript steht aber "Mads" -- und ohne diese
/// Haelfte haette die Schranke, die einen echten Namen vor einem
/// aehnlich klingenden Woerterbuch-Eintrag rettet, an jedem Bindestrich ein
/// Loch.
#[test]
fn ein_doppelname_liefert_auch_seine_haelften() {
    let teile = context_name_parts("Mads-Peter Verlin | nordwerk");
    assert!(teile.contains(&"Mads-Peter".to_string()));
    assert!(teile.contains(&"Mads".to_string()));
    assert!(teile.contains(&"Peter".to_string()));
    assert!(teile.contains(&"Verlin".to_string()));
    // Die Organisation hinter dem Strich gehoert nicht zum Namen.
    assert!(!teile.iter().any(|teil| teil == "nordwerk"));
}

/// Ein Doppelname aus dem KALENDERTEXT schuetzt seine Haelften genauso.
///
/// Der strukturierte Weg (`context_name_parts`) zerlegte "Zirk-Peter" seit dem
/// 03.09.2026, der menschengeschriebene Kalendertext nicht. Damit hing die
/// Immunitaet eines Namens daran, ob er in der Teilnehmerliste stand oder im
/// Titel -- obwohl in beiden Faellen ein Mensch ihn getippt hat.
#[test]
fn ein_doppelname_im_kalendertext_schuetzt_seine_haelften() {
    let liste = ["Zerk".to_string()];
    let text = "wir fragen Zirk heute nochmal dazu";

    // Der Kalendertitel nennt "Zirk-Peter" -- "Zirk" ist damit belegt.
    let geschuetzt = [session_mit_text("a", text, "Termin mit Zirk-Peter")];
    let found = scan(&geschuetzt, &liste, &[], &Options::default());
    assert!(
        !found.iter().any(|vorschlag| vorschlag.alias == "Zirk"),
        "Haelfte eines Doppelnamens als Verhoerung vorgeschlagen: {:?}",
        pairs(&found)
    );

    // Die Gegenprobe, und sie ist der Grund, dass der Test etwas aussagt:
    // derselbe Aufbau ohne den Doppelnamen im Titel -- jetzt kommt der
    // Vorschlag. Ohne diese Zeile waere oben auch dann gruen, wenn "Zirk"
    // ueberhaupt nie Kandidat werden koennte.
    let ungeschuetzt = [session_mit_text("a", text, "Termin mit Peter")];
    let auch = scan(&ungeschuetzt, &liste, &[], &Options::default());
    assert!(
        pairs(&auch).contains(&("Zerk".into(), "Zirk".into())),
        "Test taugt nicht: ohne Anker entsteht der Vorschlag gar nicht: {:?}",
        pairs(&auch)
    );
}

/// Alle fuenf Striche zerlegen gleich.
///
/// Vorher taten sie es nicht: der ASCII-Bindestrich hielt das Wort zusammen
/// und lieferte (im Kalendertext) NUR die ganze Form, der Halbgeviertstrich
/// trennte und lieferte NUR die Haelften. Welche Taste jemand getroffen hat,
/// entschied damit ueber die Immunitaet.
#[test]
fn jeder_strich_liefert_ganze_form_und_haelften() {
    for strich in ['-', '\u{2010}', '\u{2011}', '\u{2013}', '\u{2014}'] {
        let text = format!("Termin mit Zirk{strich}Peter");
        let teile = context_text_terms(&text);
        assert!(
            teile.contains(&format!("Zirk{strich}Peter")),
            "ganze Form fehlt bei {strich:?}: {teile:?}"
        );
        assert!(
            teile.contains(&"Zirk".to_string()),
            "Haelfte fehlt bei {strich:?}: {teile:?}"
        );
        assert!(
            teile.contains(&"Peter".to_string()),
            "Haelfte fehlt bei {strich:?}: {teile:?}"
        );
    }
}

/// Ein Doppelname schuetzt seine Haelfte auch dort, wo es zaehlt: am
/// Kandidaten, nicht nur im Zerleger.
///
/// `ein_doppelname_liefert_auch_seine_haelften` prueft die Zerlegung. Dass die
/// Zerlegung auch wirklich in die Immunitaet gelangt, sagt sie nicht -- dieser
/// Test tut es.
#[test]
fn die_haelfte_eines_doppelnamens_wird_nie_zur_verhoerung() {
    let liste = ["Zerk".to_string()];
    let text = "wir fragen Zirk heute nochmal dazu";

    let geschuetzt = [session("a", text, &["Zirk-Peter Verlin | nordwerk"])];
    let found = scan(&geschuetzt, &liste, &[], &Options::default());
    assert!(
        !found.iter().any(|vorschlag| vorschlag.alias == "Zirk"),
        "Haelfte eines Teilnehmernamens geopfert: {:?}",
        pairs(&found)
    );

    // Gegenprobe: derselbe Aufbau, aber der Teilnehmer heisst anders.
    let ungeschuetzt = [session("a", text, &["Peter Verlin | nordwerk"])];
    let auch = scan(&ungeschuetzt, &liste, &[], &Options::default());
    assert!(
        pairs(&auch).contains(&("Zerk".into(), "Zirk".into())),
        "Test taugt nicht: ohne Anker entsteht der Vorschlag gar nicht: {:?}",
        pairs(&auch)
    );
}

/// Eine Mailadresse IN DER NAMENSLISTE schuetzt ihren Traeger.
///
/// Bis zum 03.09.2026 lieferte `context_name_parts` fuer eine Mailadresse eine
/// leere Liste -- richtig fuer die ZIELE, falsch fuer die IMMUNITAET. Ein
/// Teilnehmer, der nur mit Adresse eingeladen war, hatte damit keinen Schutz,
/// und sein Name war wieder Freiwild fuer einen aehnlich klingenden
/// Woerterbuch-Eintrag.
///
/// WAS DIESER TEST NICHT SAGT, und das ist wichtig, weil er es bis zum
/// 03.09.2026 zu sagen schien: dass irgendjemand eine Adresse in diese Liste
/// legt. Er schreibt sie selbst hinein. Der Lader in der Oberflaeche tat das
/// NICHT -- seine Abfrage nahm nur `human.name` und `display_name`, die
/// Kalenderteilnehmer nur `name`. Der Fall aus dem Auftrag war also gruen
/// geprueft und im laufenden Programm offen. Ein Test, den eine andere
/// Bedingung traegt als die, um die es geht.
///
/// Dass die Adresse wirklich ankommt, pruefen jetzt
/// `apps/desktop/src/stt/proposals.sql.test.ts` (gegen eine echte Datenbank)
/// und `apps/desktop/src/stt/proposals.test.ts` (Kalenderteilnehmer).
#[test]
fn ein_teilnehmer_nur_mit_mailadresse_ist_trotzdem_belegt() {
    let liste = ["Merlin".to_string()];
    let text = "da hat Verlin uns nochmal was geschickt ja";

    let geschuetzt = [session_mit_adressen("a", text, &["mads.verlin@nordwerk.example"])];
    let found = scan(&geschuetzt, &liste, &[], &Options::default());
    assert!(
        !found.iter().any(|vorschlag| vorschlag.alias == "Verlin"),
        "Name aus der Mailadresse nicht geschuetzt: {:?}",
        pairs(&found)
    );

    // Gegenprobe: eine Sammeladresse ohne seinen Namen -- jetzt faellt er.
    let ungeschuetzt = [session_mit_adressen("a", text, &["kontakt@nordwerk.example"])];
    let auch = scan(&ungeschuetzt, &liste, &[], &Options::default());
    assert!(
        pairs(&auch).contains(&("Merlin".into(), "Verlin".into())),
        "Test taugt nicht: ohne Anker entsteht der Vorschlag gar nicht: {:?}",
        pairs(&auch)
    );
}

/// Eine Mailadresse begruendet auch in NETZ 1 keine richtige Schreibweise.
///
/// Der Blocker vom 03.09.2026 22:30. Die beiden Tests darunter und darueber
/// blieben gruen, obwohl das Loch offenstand: sie tragen je EINE Sitzung, und
/// Netz 1 steigt vorher an `min_sessions = 2` aus. Zum fuenften Mal in dieser
/// Strecke hielt eine ANDERE Bedingung einen Test gruen -- deshalb steht hier
/// ausdruecklich der Aufbau mit ZWEI Sitzungen.
///
/// Der Weg ins Loch fuehrte nicht ueber die Ziele von Netz 2 (die beruehren
/// `context_emails` nie), sondern ueber die Anker-Menge: `known_terms` warf
/// Adressteile und Namen in EINE Menge, und Netz 1 waehlte seine kanonische
/// Schreibweise genau daraus. Die Trennung der Felder half nicht, weil die
/// Mengen dahinter wieder zusammenliefen.
///
/// WIRD ROT, wenn die Kanonisierung in `scan_recurrence` wieder aus der
/// weiten Menge (`known.immun`) waehlt.
/// LAESST DURCH: ob die Immunitaet noch greift -- dafuer steht der zweite
/// Block unten und `eine_mailadresse_wird_nie_zum_ziel`.
#[test]
fn eine_mailadresse_begruendet_auch_ueber_zwei_gespraeche_kein_ziel() {
    let mit_adresse = [
        session_mit_adressen("a", "da hat Verlin uns geantwortet ja", &[
            "verlin@nordwerk.example",
        ]),
        session_mit_adressen("b", "und dann hat Werlin geantwortet ja", &[]),
    ];
    let found = scan(&mit_adresse, &[], &[], &Options::default());
    assert!(
        found.is_empty(),
        "Mailadresse hat die richtige Schreibweise begruendet: {:?}",
        pairs(&found)
    );

    // Gegenprobe eins: derselbe Aufbau mit dem Wert als NAME liefert den
    // Vorschlag sehr wohl. Ohne sie bewiese der Teil oben nur, dass die
    // Gruppe gar nicht zustande kommt.
    let als_name = [
        session("a", "da hat Verlin uns geantwortet ja", &["Verlin"]),
        session("b", "und dann hat Werlin geantwortet ja", &[]),
    ];
    assert!(
        pairs(&scan(&als_name, &[], &[], &Options::default()))
            .contains(&("Verlin".into(), "Werlin".into())),
        "Test taugt nicht: der Vorschlag entsteht auch als Name nicht"
    );

    // Gegenprobe zwei: die Immunitaet aus derselben Adresse bleibt. Sie ist
    // der Grund, warum das Feld ueberhaupt gelesen wird -- ein Fix, der sie
    // mitnimmt, waere schlechter als der Fehler.
    let geschuetzt = [
        session_mit_adressen("a", "da hat Verlin uns geantwortet ja", &[
            "verlin@nordwerk.example",
        ]),
        session_mit_adressen("b", "und Verlin hat auch geantwortet ja", &[]),
    ];
    let mit_liste = scan(
        &geschuetzt,
        &["Merlin".to_string()],
        &[],
        &Options::default(),
    );
    assert!(
        !mit_liste.iter().any(|vorschlag| vorschlag.alias == "Verlin"),
        "die Adresse schuetzt den Namen nicht mehr: {:?}",
        pairs(&mit_liste)
    );
}

/// DASSELBE ueber den freien Text -- und das war bis zum 04.09.2026 offen.
///
/// Eine Adresse steht nicht nur im Teilnehmerfeld. Sie steht auch mitten in
/// einer Notiz oder Kalenderbeschreibung, und dort nahm `context_text_terms`
/// jedes grossgeschriebene Wort ab vier Zeichen und machte es zum ANKER.
/// Der strukturierte Weg war laengst gehaertet -- aber SAEMTLICHE Adresstests
/// speisten ueber `context_emails` ein, und deshalb hat die Luecke vier
/// Pruefrunden ueberlebt.
///
/// ACHTUNG, hier steckt der Test selbst in der Falle: der Lokalteil ist
/// bewusst GROSSGESCHRIEBEN. Mit "verlin@nordwerk.example" waere der Test auch
/// ohne den Fix gruen, denn ein kleingeschriebenes Wort wurde nie zum Anker
/// -- er pruefte dann die Grossschreibungs-Schranke statt der Adress-Regel.
///
/// WIRD ROT, wenn der Lokalteil einer Adresse im freien Text wieder ankert.
/// LAESST DURCH, was mit dem Domainteil passiert.
#[test]
fn eine_adresse_im_freien_text_begruendet_kein_ziel() {
    let mit_adresse = [
        session_mit_text(
            "a",
            "da hat Verlin uns geantwortet ja",
            "Rueckfragen an Verlin@nordwerk.example",
        ),
        session("b", "und dann hat Werlin geantwortet ja", &[]),
    ];
    let found = scan(&mit_adresse, &[], &[], &Options::default());
    assert!(
        found.is_empty(),
        "die Adresse im freien Text hat die richtige Schreibweise begruendet: {:?}",
        pairs(&found)
    );

    // Gegenprobe eins, und ohne sie sagt der Block darueber nichts: derselbe
    // Aufbau mit dem Wort als NAME im Text liefert den Vorschlag sehr wohl.
    // Ein Fix, der den Kalendertext pauschal entwertet, faellt hier durch.
    let als_name = [
        session_mit_text(
            "a",
            "da hat Verlin uns geantwortet ja",
            "Rueckfragen an Verlin nordwerk",
        ),
        session("b", "und dann hat Werlin geantwortet ja", &[]),
    ];
    assert!(
        pairs(&scan(&als_name, &[], &[], &Options::default()))
            .contains(&("Verlin".into(), "Werlin".into())),
        "Test taugt nicht: der Vorschlag entsteht auch als Name nicht"
    );

    // Gegenprobe zwei: die IMMUNITAET aus derselben Adresse muss bleiben. Sie
    // ist der teure Teil -- ein Fix, der sie mitnimmt, opfert einen echten
    // Namen an einen aehnlich klingenden Woerterbuch-Eintrag und waere
    // schlechter als der Fehler, den er behebt.
    let geschuetzt = [
        session_mit_text(
            "a",
            "da hat Verlin uns geantwortet ja",
            "Rueckfragen an Verlin@nordwerk.example",
        ),
        session_mit_text(
            "b",
            "und Verlin hat auch geantwortet ja",
            "Rueckfragen an Verlin@nordwerk.example",
        ),
    ];
    let mit_liste = scan(
        &geschuetzt,
        &["Merlin".to_string()],
        &[],
        &Options::default(),
    );
    assert!(
        !mit_liste.iter().any(|vorschlag| vorschlag.alias == "Verlin"),
        "die Adresse im freien Text schuetzt den Namen nicht mehr: {:?}",
        pairs(&mit_liste)
    );
}

/// Der DOMAINTEIL wird nie zum Ziel.
///
/// Dieselbe Begruendung wie auf dem strukturierten Weg: dort stehen "gmail",
/// "outlook" und "web" so oft wie Firmennamen. Ein Domainname, den ein Mensch
/// nie als Wort gemeint hat, darf keine Schreibweise begruenden.
///
/// WIRD ROT, wenn der Teil hinter dem Klammeraffen wieder mitgelesen wird.
#[test]
fn der_domainteil_einer_adresse_im_freien_text_ankert_nicht() {
    let sessions = [
        session_mit_text(
            "a",
            "da hat uns Nordwerk gleich geantwortet ja",
            // Der Domainteil ist GROSS geschrieben und vier Zeichen lang --
            // genau die Form, die `context_text_terms` sonst als Anker nimmt.
            // Kleingeschrieben traefe der Test seinen Gegenstand nicht mehr.
            "Rueckfragen an mads@Nordwerk.example",
        ),
        session("b", "und dann hat Nordwerck geantwortet ja", &[]),
    ];
    let found = scan(&sessions, &[], &[], &Options::default());
    assert!(
        found.is_empty(),
        "der Domainteil hat die richtige Schreibweise begruendet: {:?}",
        pairs(&found)
    );
}

/// EIN STUECK OHNE LEERZEICHEN VERLIERT SEINE NACHBARN NICHT.
///
/// Die Haertung vom 04.09.2026 trennte Anker und Immunitaet an
/// `split_once('@')` und warf den Teil hinter dem Klammeraffen weg -- nur ist
/// das nicht der Domainteil, sondern der GANZE Rest des Leerraum-Stuecks.
/// Solange eine Adresse von Leerzeichen umgeben ist, faellt das nicht auf.
///
/// Der Hauptpfad sieht aber genau das nicht: seit dem Ausbau des
/// Titelschnitts geht die Notiz als rohes, minifiziertes JSON in den Kontext,
/// und darin steht fast kein Leerzeichen. Ein einziges Stueck sieht dann so
/// aus:
///
/// ```text
/// verlin@nordwerk.example"}]},{"type":"text","text":"Verlin
/// ```
///
/// Alles ab dem Klammeraffen verschwand -- "Verlin" verlor damit Anker UND
/// Immunitaet und war wieder als Verhoerung eines aehnlich klingenden
/// Woerterbuch-Eintrags vorschlagbar. Vor der Haertung war er beides. Ein Fix,
/// der die Immunitaet mitnimmt, ist schlechter als der Fehler, den er behebt:
/// er opfert einen echten Namen an "Merlin".
///
/// WIRD ROT, wenn ein Wort neben einer Adresse im selben leerzeichenfreien
/// Stueck seine Immunitaet verliert -- in allen drei Formen, die im echten
/// Bestand vorkommen (minifiziertes JSON, komma-geklebte Adressen, ein
/// fuehrendes "@" ohne Lokalteil).
/// LAESST DURCH, ob ein solches Stueck ankert; dafuer steht der Block ganz
/// unten und der Test darueber.
#[test]
fn ein_stueck_ohne_leerzeichen_verliert_seine_nachbarn_nicht() {
    // Ein Nachbar im selben Stueck bleibt immun -- drei Formen, ein Muster.
    for (form, kontext) in [
        (
            "minifiziertes JSON",
            r#"Kontakt:mads@nordwerk.example"}]},{"type":"text","text":"Verlin"#,
        ),
        ("komma-geklebt", "mads@nordwerk.example,Verlin@nordwerk.example"),
        ("fuehrendes @ ohne Lokalteil", "@Verlin"),
    ] {
        let sessions = [
            session_mit_text("a", "da hat Verlin uns geantwortet ja", kontext),
            session_mit_text("b", "und Verlin hat auch geantwortet ja", kontext),
        ];
        let found = scan(
            &sessions,
            &["Merlin".to_string()],
            &[],
            &Options::default(),
        );
        assert!(
            !found.iter().any(|vorschlag| vorschlag.alias == "Verlin"),
            "{form}: der Name neben der Adresse hat seine Immunitaet verloren: {:?}",
            pairs(&found)
        );
    }

    // Die Gegenprobe, und ohne sie sagt der Block darueber nichts: derselbe
    // Aufbau OHNE Klammeraffen im Stueck liefert den Vorschlag sehr wohl.
    // Ein Fix, der jedes Stueck pauschal immun macht, faellt hier durch.
    let ohne_adresse = [
        session_mit_text("a", "da hat Verlin uns geantwortet ja", "Notiz ohne Adresse"),
        session_mit_text("b", "und Verlin hat auch geantwortet ja", "Notiz ohne Adresse"),
    ];
    assert!(
        pairs(&scan(
            &ohne_adresse,
            &["Merlin".to_string()],
            &[],
            &Options::default()
        ))
        .contains(&("Merlin".into(), "Verlin".into())),
        "Test taugt nicht: der Vorschlag entsteht auch ohne Adresse nicht"
    );

    // Und die andere Richtung, die den billigen Preis festschreibt: ein Wort
    // aus einem Stueck MIT Klammeraffen ankert nicht -- auch dann nicht, wenn
    // es gross geschrieben neben der Adresse steht. Wer die Immunitaet
    // repariert, indem er das ganze Stueck in die Anker-Menge kippt, macht
    // aus "Verlin" wieder eine Schreibweise, gegen die jedes aehnlich
    // klingende Wort zur Verhoerung erklaert wird.
    let als_anker = [
        session_mit_text(
            "a",
            "da hat Verlin uns geantwortet ja",
            r#"mads@nordwerk.example"}]},{"text":"Verlin"#,
        ),
        session("b", "und dann hat Werlin geantwortet ja", &[]),
    ];
    let found = scan(&als_anker, &[], &[], &Options::default());
    assert!(
        found.is_empty(),
        "das Stueck mit der Adresse hat die richtige Schreibweise begruendet: {:?}",
        pairs(&found)
    );
}

/// OHNE JEDEN BELEG KEIN VORSCHLAG -- der Falsifikator aus dem
/// Abnahmekriterium ("Vorschlaege auf einem Gespraech ohne Teilnehmer und
/// ohne Termin").
///
/// Es gibt dafuer bewusst KEIN eigenes Gatter, und das ist ein Befund, kein
/// Versaeumnis: die Zusage haengt an zwei Waechtern, die schon da sind. Netz 1
/// bricht ab, wenn keine Schreibweise der Gruppe in der Anker-Menge steht
/// (`else { continue }` in `scan_recurrence`); Netz 2 steigt bei leerer
/// Ziel-Liste aus. Ein zusaetzliches Gatter auf "hat dieses Gespraech
/// Teilnehmer oder Termin" waere sogar FALSCH -- es wuerde die
/// Woerterbuchliste abwuergen, die fuer jedes Gespraech ein
/// menschengeschriebener Beleg ist. Genau das ist am 04.09.2026 gebaut und
/// nach elf roten Tests wieder zurueckgebaut worden.
///
/// WIRD ROT, wenn eines der beiden Netze ohne Beleg wieder raet -- etwa durch
/// einen Rueckfall auf die haeufigste Schreibweise.
/// LAESST DURCH, ob ein Beleg aus dem RICHTIGEN Feld kommt; dafuer stehen die
/// Adress-Tests.
#[test]
fn ohne_jeden_beleg_entsteht_kein_vorschlag() {
    // Kein Teilnehmer, keine Adresse, kein Kalendertext, kein Woerterbuch.
    let ohne_alles = [
        session("a", "wir haben Grandpfeil gefragt und", &[]),
        session("b", "dann hat Grandfall geantwortet und", &[]),
    ];
    let found = scan(&ohne_alles, &[], &[], &Options::default());
    assert!(
        found.is_empty(),
        "ohne jeden Beleg wurde geraten: {:?}",
        pairs(&found)
    );

    // Der positive Zwilling, und er ist hier Pflicht: dieselben Gespraeche
    // mit EINEM Woerterbuch-Eintrag liefern das Paar. Ohne ihn bestuende der
    // Block darueber auch dann, wenn die Gruppe gar nicht zustande kaeme --
    // dann pruefte er den Klangvergleich statt der Beleg-Pflicht.
    assert!(
        pairs(&scan(
            &ohne_alles,
            &["Grandpfeil".into()],
            &[],
            &Options::default()
        ))
        .contains(&("Grandpfeil".into(), "Grandfall".into())),
        "Test taugt nicht: das Paar entsteht auch mit Beleg nicht"
    );

    // Und NETZ 2 einzeln: ein einziges Gespraech erreicht die
    // Sitzungsschwelle von Netz 1 nie, hier laeuft also nur Netz 2. Ohne
    // Kontext und ohne Liste hat es kein Ziel.
    let ein_gespraech = [session("a", "da hat Grandfall uns geantwortet ja", &[])];
    let netz_zwei = scan(&ein_gespraech, &[], &[], &Options::default());
    assert!(
        netz_zwei.is_empty(),
        "Netz 2 hat ohne Ziel vorgeschlagen: {:?}",
        pairs(&netz_zwei)
    );

    // Und auch dieser Block braucht seinen positiven Zwilling -- er war der
    // einzige ohne. Ohne ihn bestuende er auch dann, wenn Netz 2 an diesem
    // Aufbau gar nicht bis zum Klangvergleich kaeme; er pruefte dann die
    // Kandidatenauswahl statt der Beleg-Pflicht.
    assert!(
        pairs(&scan(
            &ein_gespraech,
            &["Grandpfeil".into()],
            &[],
            &Options::default()
        ))
        .contains(&("Grandpfeil".into(), "Grandfall".into())),
        "Test taugt nicht: Netz 2 findet das Paar auch mit Beleg nicht"
    );
}

/// Die beiden Rollen von NETZ 1, an einem Aufbau, der Netz 1 wirklich
/// erreicht.
///
/// Zwei Mutanten ueberlebten am 03.09.2026 die ganze Testdatei, weil kein
/// einziger Fall Netz 1 in die Lage brachte, einen Alias zu SPERREN oder
/// einen Woerterbuch-Eintrag als Anker zu BRAUCHEN:
///
/// * Die Immunitaets-Sperre in `scan_recurrence` durfte auf die enge Menge
///   umgestellt werden -- die Adresse haette ihren Namen nicht mehr
///   geschuetzt, und kein Test wurde rot.
/// * Die Woerterbuch-Eintraege durften aus der Anker-Menge fallen -- Netz 1
///   haette keine Gruppe mehr kanonisieren koennen, und kein Test wurde rot.
///
/// Der Aufbau ist deshalb bewusst so gewaehlt, dass BEIDE Zeilen tragen: die
/// Gruppe entsteht aus zwei Gespraechen (`min_sessions`), ihre richtige
/// Schreibweise steht ausschliesslich im Woerterbuch, und der Alias ist genau
/// das Wort, das die Adresse schuetzen soll.
///
/// WIRD ROT, wenn die Woerterbuch-Eintraege den Anker verlieren (erster Block)
/// oder die Immunitaet in Netz 1 die Adressen nicht mehr sieht (zweiter).
/// LAESST DURCH: Netz 2 -- das steht in den beiden Tests darunter.
#[test]
fn in_netz_eins_ankert_das_woerterbuch_und_die_adresse_schuetzt() {
    // "Meinerz" statt "Verlin": Netz 1 gruppiert nur, was schon im ERSTEN
    // Laut uebereinstimmt (Koelner Code, `by_first`). "Verlin" und "Merlin"
    // liegen dort in verschiedenen Eimern -- jeder Fund zu diesem Paar kommt
    // aus Netz 2, und ein Netz-1-Test daran waere wieder eine Attrappe.
    let liste = ["Meiners".to_string()];
    let ohne_adresse = [
        session_nur_netz_eins("a", "da hat Meinerz uns geantwortet ja", &[]),
        session_nur_netz_eins("b", "und Meiners hat auch geantwortet ja", &[]),
    ];
    assert!(
        pairs(&scan(&ohne_adresse, &liste, &[], &Options::default()))
            .contains(&("Meiners".into(), "Meinerz".into())),
        "Netz 1 ankert nicht mehr am Woerterbuch -- ohne diesen Vorschlag \
         sagt der zweite Block nichts"
    );

    let mit_adresse = [
        session_nur_netz_eins("a", "da hat Meinerz uns geantwortet ja", &[
            "meinerz@nordwerk.example",
        ]),
        session_nur_netz_eins("b", "und Meiners hat auch geantwortet ja", &[]),
    ];
    let found = scan(&mit_adresse, &liste, &[], &Options::default());
    assert!(
        !found.iter().any(|vorschlag| vorschlag.alias == "Meinerz"),
        "die Adresse schuetzt den Namen in Netz 1 nicht: {:?}",
        pairs(&found)
    );
}

/// Die Mailadresse liefert AUSSCHLIESSLICH Immunitaet, nie ein Ziel.
///
/// Waere der Adress-Teil auch ein Ziel, wuerde aus "buchhaltung@nordwerk.example"
/// das Zielwort "buchhaltung" -- und ein aehnlich klingendes Wort im
/// Transkript bekaeme den Vorschlag, kuenftig durch eine Postfachadresse
/// ersetzt zu werden.
///
/// ⚠️ Dieser Test traegt EINE Sitzung und sagt deshalb nur etwas ueber Netz 2:
/// Netz 1 steigt vorher an `min_sessions` aus. Fuer Netz 1 steht
/// `eine_mailadresse_begruendet_auch_ueber_zwei_gespraeche_kein_ziel` darueber.
#[test]
fn eine_mailadresse_wird_nie_zum_ziel() {
    let sessions = [session_mit_adressen(
        "a",
        "und dann hat Werlin das nochmal erklaert ja",
        &["verlin@nordwerk.example"],
    )];
    let found = scan(&sessions, &[], &[], &Options::default());
    assert!(
        found.is_empty(),
        "Mailadresse als Ziel benutzt: {:?}",
        pairs(&found)
    );
}

/// DER RANDFALL, an dem die Zusage bis zum 03.09.2026 nicht galt: ein Wert im
/// Adressfeld OHNE "@".
///
/// Bis dahin lagen Namen und Adressen in EINER Liste, und die Herkunft wurde
/// aus dem Inhalt zurueckgerechnet -- "traegt ein @" hiess Adresse. Ein
/// Kalendereintrag `{email: "Verlin"}` war danach von einem Namen nicht mehr
/// zu unterscheiden und wurde zum ZIEL: der Lauf schlug vor, "Werlin" kuenftig
/// durch "Verlin" zu ersetzen, auf Grundlage einer Quelle, die genau das nie
/// tun darf. Die Herkunft steht jetzt im FELD, und ein Feld kann man nicht
/// falsch raten.
///
/// Wird rot, wenn `context_emails` je ein Ziel liefert.
/// Laesst durch: ein Adresswert, der faelschlich in `context_names` landet --
/// dafuer steht der Waechter in `context_name_parts` und der Lader-Test
/// drueben in `proposals.sql.test.ts`.
#[test]
fn ein_adressfeld_ohne_klammeraffen_wird_trotzdem_nie_zum_ziel() {
    let text = "und dann hat Werlin das nochmal erklaert ja";
    let sessions = [session_mit_adressen("a", text, &["Verlin"])];
    let found = scan(&sessions, &[], &[], &Options::default());
    assert!(
        found.is_empty(),
        "Adressfeld ohne @ als Ziel benutzt: {:?}",
        pairs(&found)
    );

    // Die Gegenprobe, ohne die der Test nichts wert waere: derselbe Wert als
    // NAME ist sehr wohl ein Ziel. Faende er auch hier nichts, bewiese der
    // Teil oben nur, dass der Klangvergleich gar nicht greift.
    let als_name = [session("a", text, &["Verlin"])];
    assert!(
        pairs(&scan(&als_name, &[], &[], &Options::default()))
            .contains(&("Verlin".into(), "Werlin".into())),
        "Test taugt nicht: der Vorschlag entsteht auch als Name nicht"
    );

    // Und die Immunitaet bleibt: derselbe Wert im Adressfeld schuetzt den
    // Namen im Transkript vor einem aehnlich klingenden Woerterbuch-Eintrag.
    let liste = ["Merlin".to_string()];
    let geschuetzt = [session_mit_adressen(
        "a",
        "da hat Verlin uns nochmal was geschickt ja",
        &["Verlin"],
    )];
    let mit_liste = scan(&geschuetzt, &liste, &[], &Options::default());
    assert!(
        !mit_liste.iter().any(|vorschlag| vorschlag.alias == "Verlin"),
        "Adressfeld ohne @ stiftet keine Immunitaet: {:?}",
        pairs(&mit_liste)
    );
}

/// Ein gewoehnliches deutsches Wort ist nie eine RICHTIGE SCHREIBWEISE --
/// auch nicht in Netz 2.
///
/// Netz 1 hielt diesen Waechter, Netz 2 nicht. Die Asymmetrie kostete doppelt:
/// das Woerterbuch bekaeme einen Eintrag wie `Webseite => Website`, und der
/// Nachlauf ersetzt danach in JEDEM kuenftigen Gespraech jedes richtig
/// geschriebene Wort.
///
/// Das Paar hier ist mit Bedacht gewaehlt: "Kalender" steht in der
/// Waechterliste, "Kalendar" nicht -- der Kandidat kommt also durch, und
/// verworfen wird allein wegen des ZIELS. Ein Paar, bei dem schon der Kandidat
/// haengenbleibt, saegte ueber den Waechter am Ziel nichts aus.
#[test]
fn ein_alltagswort_wird_in_netz_zwei_nicht_zum_ziel() {
    let sessions = [session(
        "a",
        "wir haben den Kalendar nochmal durchgesehen ja",
        &[],
    )];
    let found = scan(&sessions, &["Kalender".into()], &[], &Options::default());
    assert!(
        !found
            .iter()
            .any(|vorschlag| vorschlag.canonical == "Kalender"),
        "Alltagswort als richtige Schreibweise vorgeschlagen: {:?}",
        pairs(&found)
    );

    // Gegenprobe: derselbe Aufbau mit einem Ziel, das KEIN Alltagswort ist.
    // Das ist die einzige Aenderung -- und der Vorschlag kommt.
    let mit_namen = [session(
        "a",
        "wir haben den Merkantin nochmal durchgesehen ja",
        &[],
    )];
    let auch = scan(&mit_namen, &["Merkentin".into()], &[], &Options::default());
    assert!(
        pairs(&auch).contains(&("Merkentin".into(), "Merkantin".into())),
        "Test taugt nicht: das Netz findet in diesem Aufbau ueberhaupt nichts: {:?}",
        pairs(&auch)
    );
}

/// Der Vergleichsschluessel kennt dasselbe Leerraum-Alphabet wie die
/// Oberflaeche.
///
/// Die eine gemessene Abweichung zwischen den beiden Sprachen: JavaScripts
/// `\s` deckt `U+FEFF` mit ab, Rusts `char::is_whitespace` nicht. Ein aus einer
/// Tabelle kopierter Begriff traegt dieses Zeichen unsichtbar mit -- und ein
/// verworfenes Paar kaeme mit zwei verschiedenen Schluesseln wieder.
///
/// Die Gegenseite dieser Zusage steht in `dictionary-entry.test.ts`; ein
/// gemeinsamer Pruefstand fuer beide Sprachen existiert nicht, also stehen
/// zwei Tests fuer eine Regel, und beide nennen einander.
#[test]
fn der_schluessel_kennt_das_byte_reihenfolge_zeichen() {
    assert_eq!(key("Grand\u{FEFF}pfeil"), "grand pfeil");
    assert_eq!(key("\u{FEFF}Grandpfeil"), "grandpfeil");
    assert_eq!(key("  Nordwerk\tAnlagenbau  "), "nordwerk anlagenbau");
}

/// Die Schwellen gelten fuer das Paar, das WIRKLICH vorgeschlagen wird.
///
/// Die Gruppenbildung ist Einfachverkettung: A passt zu B, B passt zu C, also
/// liegen alle drei zusammen -- auch wenn A und C weit auseinander sind. Ohne
/// die Nachpruefung ginge daraus ein Vorschlag hervor, dessen Abstand nie
/// geprueft wurde.
#[test]
fn die_verkettung_bildet_die_gruppe_aber_rechtfertigt_nicht_das_paar() {
    let optionen = Options::default();
    // Die Kette: jedes Glied haelt die Schwellen, die beiden Enden nicht.
    // Gemessen: je 2 Buchstaben bei erlaubten 2, ueber Eck 3 bei erlaubten 2.
    //
    // Alle drei beginnen mit demselben Laut. Das ist kein Zufall, sondern
    // Bedingung: die Gruppenbildung sortiert erst nach der ERSTEN Ziffer des
    // Koelner-Codes, und ein frueherer Anlauf mit "Merkentin"/"Serkantin"
    // bestand nur deshalb, weil die beiden nie im selben Eimer landeten --
    // die Gruppe entstand gar nicht, und der Test sagte ueber die
    // Nachpruefung nichts aus.
    assert!(sounds_alike("Sarkan", "Serken", &optionen));
    assert!(sounds_alike("Serken", "Serkins", &optionen));
    assert!(
        !sounds_alike("Sarkan", "Serkins", &optionen),
        "Test taugt nicht: die beiden Enden passen ohnehin zusammen"
    );
    assert_eq!(
        encode("Sarkan").chars().next(),
        encode("Serkins").chars().next(),
        "Test taugt nicht: die beiden landen in verschiedenen Eimern"
    );

    let sessions = [
        session("a", "wir haben Sarkan gefragt und Serken gehoert", &[]),
        session("b", "dann hat Serkins geantwortet und Serken auch", &[]),
    ];
    let found = scan(&sessions, &["Sarkan".into()], &[], &optionen);
    assert!(
        !pairs(&found).contains(&("Sarkan".into(), "Serkins".into())),
        "ueber Eck vorgeschlagen, ohne die Schwellen zu pruefen: {:?}",
        pairs(&found)
    );
    // Das nahe Glied bleibt selbstverstaendlich im Ergebnis.
    assert!(pairs(&found).contains(&("Sarkan".into(), "Serken".into())));
}

// -------------------------------------------------------------------------
// Netz 2
// -------------------------------------------------------------------------

#[test]
fn ein_name_aus_dem_kontext_faengt_seine_verhoerung() {
    let sessions = [session(
        "a",
        "und dann hat Werlin das nochmal erklaert ja",
        &["Mads Verlin | nordwerk"],
    )];
    let found = scan(&sessions, &[], &[], &Options::default());
    assert!(
        pairs(&found).contains(&("Verlin".into(), "Werlin".into())),
        "erwartet Verlin <- Werlin, bekommen: {:?}",
        pairs(&found)
    );
}

/// Der Falsifikator aus dem Auftrag: ohne Teilnehmer und ohne Kalendertermin
/// keine Vorschlaege aus Netz 2.
#[test]
fn ohne_kontext_kein_vorschlag_aus_netz_zwei() {
    let sessions = [session(
        "a",
        "und dann hat Werlin das nochmal erklaert ja",
        &[],
    )];
    let found = scan(&sessions, &[], &[], &Options::default());
    assert!(found.is_empty(), "unerwartet: {:?}", pairs(&found));
}

/// Der Falsifikator aus dem Auftrag: ein Wort mit hoher GEMESSENER Sicherheit
/// wird nicht vorgeschlagen.
#[test]
fn hohe_gemessene_sicherheit_haelt_das_wort_aus_dem_netz() {
    let sicher = ScanSession {
        session_id: "a".into(),
        words: vec![
            word("und"),
            word("dann"),
            word("hat"),
            ScanWord {
                text: " Werlin".into(),
                measured_confidence: Some(0.999),
            },
            word("das"),
            word("erklaert"),
        ],
        context_names: vec!["Mads Verlin | nordwerk".into()],
        context_emails: vec![],
        generated_title: String::new(),
        context_text: String::new(),
    };
    let found = scan(&[sicher], &[], &[], &Options::default());
    assert!(found.is_empty(), "unerwartet: {:?}", pairs(&found));
}

/// `None` heisst "nicht gemessen", NICHT "sicher".
///
/// Gemessen am 03.09.2026: acht von neun lebenden Transkripten tragen
/// ueberhaupt keinen Wert. Ein Netz, das die Leerstelle wie Sicherheit liest,
/// waere auf 89 % des Bestands blind.
#[test]
fn fehlende_sicherheit_schliesst_ein_wort_nicht_aus() {
    let ungemessen = ScanSession {
        session_id: "a".into(),
        words: vec![
            word("und"),
            word("dann"),
            word("hat"),
            ScanWord {
                text: " Werlin".into(),
                measured_confidence: None,
            },
            word("das"),
            word("erklaert"),
        ],
        context_names: vec!["Mads Verlin | nordwerk".into()],
        context_emails: vec![],
        generated_title: String::new(),
        context_text: String::new(),
    };
    let found = scan(&[ungemessen], &[], &[], &Options::default());
    assert!(
        pairs(&found).contains(&("Verlin".into(), "Werlin".into())),
        "ungemessenes Wort faelschlich ausgeschlossen: {:?}",
        pairs(&found)
    );
}

/// Niedrige Sicherheit laesst das Wort durch -- die Gegenprobe zum Deckel.
#[test]
fn niedrige_gemessene_sicherheit_laesst_das_wort_durch() {
    let unsicher = ScanSession {
        session_id: "a".into(),
        words: vec![
            word("und"),
            word("dann"),
            word("hat"),
            ScanWord {
                text: " Werlin".into(),
                measured_confidence: Some(0.41),
            },
            word("das"),
            word("erklaert"),
        ],
        context_names: vec!["Mads Verlin | nordwerk".into()],
        context_emails: vec![],
        generated_title: String::new(),
        context_text: String::new(),
    };
    let found = scan(&[unsicher], &[], &[], &Options::default());
    assert!(
        pairs(&found).contains(&("Verlin".into(), "Werlin".into())),
        "unsicheres Wort faelschlich ausgeschlossen: {:?}",
        pairs(&found)
    );
}

/// Der Organisations-Anhang des Kalenders ist kein Name.
#[test]
fn der_teil_hinter_dem_strich_ist_kein_name() {
    assert_eq!(
        context_name_parts("Mads Verlin | nordwerk"),
        vec!["Mads".to_string(), "Verlin".to_string()]
    );
    assert!(context_name_parts("wir@nordwerk.example").is_empty());
}

/// Der Klang allein ist zu wenig Beweis -- die Lehre vom 02.09.2026, hier als
/// Test.
///
/// Das Paar ist so gewaehlt, dass die KLANG-Schranke es durchlaesst und erst
/// die SCHREIB-Schranke es verwirft. Ein Test, der schon am Klang scheitert,
/// wuerde ueber die zweite Schranke gar nichts aussagen.
#[test]
fn gleicher_klang_ohne_schreib_naehe_ist_kein_treffer() {
    let options = Options::default();
    let (links, rechts) = ("Sodas", "Suedetauss");

    // Die Klang-Schranke ist zufrieden ...
    assert!(
        levenshtein(&encode(links), &encode(rechts)) <= options.max_code_edits,
        "Test taugt nicht: schon der Klang trennt die beiden"
    );
    // ... die Schreib-Schranke nicht.
    assert!(
        levenshtein(&links.to_lowercase(), &rechts.to_lowercase())
            > max_edits_for(links, rechts, &options)
    );
    assert!(!sounds_alike(links, rechts, &options));
}

// -------------------------------------------------------------------------
// Beides
// -------------------------------------------------------------------------

/// Ablehnen ist dauerhaft: ein verworfenes Paar kommt beim naechsten Lauf
/// nicht wieder.
#[test]
fn ein_abgelehntes_paar_kommt_nicht_wieder() {
    let sessions = [
        session("a", "wir haben Grandpfeil gefragt und", &[]),
        session("b", "dann hat Grandfall geantwortet und", &[]),
    ];
    let options = Options::default();
    let ohne = scan(&sessions, &["Grandpfeil".into()], &[], &options);
    assert!(!ohne.is_empty());

    let mit = scan(
        &sessions,
        &["Grandpfeil".into()],
        &[("grandpfeil".into(), "GRANDFALL".into())],
        &options,
    );
    assert!(
        mit.is_empty(),
        "abgelehntes Paar kam wieder: {:?}",
        pairs(&mit)
    );
}

/// Was schon am Eintrag steht, wird nicht nochmal vorgeschlagen -- und auch
/// nicht andersherum.
///
/// Der zweite Teil ist der wichtigere. Ein frueherer Stand nahm die ROHE
/// Woerterbuchzeile als Kontextquelle fuer Netz 2 und schleuste damit die
/// eingetragene VERHOERUNG als gueltiges Ziel ein: der Lauf schlug vor,
/// kuenftig "Grandpfeil" durch "Grandfall" zu ersetzen -- die Umkehrung
/// genau der Entscheidung, die der Betreiber getroffen hatte. Gefunden von diesem
/// Test, nicht von einem Menschen.
#[test]
fn eine_bereits_eingetragene_verhoerung_wird_in_keiner_richtung_vorgeschlagen() {
    let sessions = [
        session("a", "wir haben Grandpfeil gefragt und", &[]),
        session("b", "dann hat Grandfall geantwortet und", &[]),
    ];
    let found = scan(
        &sessions,
        &["Grandpfeil => Grandfall".into()],
        &[],
        &Options::default(),
    );
    assert!(found.is_empty(), "unerwartet: {:?}", pairs(&found));
}

/// Die Verhoerung einer Woerterbuchzeile ist NIE ein Ziel fuer Netz 2.
///
/// Die Gegenprobe zum Test darueber, an einem Paar, das nicht schon durch die
/// Bekannt-Sperre faellt: "Sarnec" steht als Verhoerung in der Liste,
/// "Sarnek" kommt im Gespraech vor. Vorgeschlagen werden darf nur der
/// Eintrag selbst, niemals seine Verhoerung.
#[test]
fn eine_eingetragene_verhoerung_wird_nie_zur_richtigen_schreibweise() {
    // Das Paar ist mit Bedacht gewaehlt: "Sarnek" klingt wie die EINGETRAGENE
    // VERHOERUNG "Sarnec", aber NICHT wie die richtige Schreibweise
    // "Ohlandez" (gemessen: Buchstaben-Abstand 5, erlaubt 3). Ein frueherer
    // Anlauf nahm "Sarnek" -- der passte auf beide, die richtige
    // Schreibweise stand in der Zielliste vorn und gewann ohnehin, und der
    // Test ueberlebte den Mutanten, gegen den er geschrieben war.
    let sessions = [session(
        "a",
        "dann hat Sarnek uns nochmal geschrieben ja",
        &[],
    )];
    let found = scan(
        &sessions,
        &["Ohlandez => Sarnec".into()],
        &[],
        &Options::default(),
    );
    assert!(
        !found.iter().any(|proposal| proposal.canonical == "Sarnec"),
        "eingetragene Verhoerung als Ziel vorgeschlagen: {:?}",
        pairs(&found)
    );
}

/// PUNKT 1 DER PRUEFRUNDE: war die Gegenrichtung der Paar-Sperre tot oder
/// lebendig?
///
/// Sie ist TOT, und zwar aus einem Grund, den man hinschreiben kann statt ihn
/// zu vermuten. Die Ableitung in zwei Schritten:
///
/// 1. Jede richtige Schreibweise des Woerterbuchs liegt in `known`
///    (`known_terms` nimmt `canonical_terms` als Erstes auf).
/// 2. Kein Mitglied von `known` kann jemals eine Verhoerung eines Vorschlags
///    werden -- beide Netze verwerfen einen solchen Alias.
///
/// Fuer die Gegenrichtung eines bekannten Paares `A => c` muesste `A` die
/// Verhoerung sein. Nach (1) liegt `A` in `known`, nach (2) kann es keine
/// Verhoerung sein. Der Fall existiert nicht.
///
/// Dieser Test prueft GENAU DIESE INVARIANTE -- nicht den Nebeneffekt. Der
/// Vorgaenger war gruen, weil beide Schreibweisen in `known` standen, und
/// ueberlebte deshalb den Mutanten, gegen den er geschrieben war.
#[test]
fn eine_richtige_schreibweise_wird_nie_zur_verhoerung() {
    // Der Aufbau ist absichtlich der unguenstigste: die richtige Schreibweise
    // steht alphabetisch HINTER der Verhoerung, kommt also bei der Wahl des
    // kanonischen Wortes zuletzt dran.
    for kontext in ["", "Grandfall Strategie"] {
        let sessions = [
            ScanSession {
                session_id: "a".into(),
                words: " wir haben Grandpfeil gefragt und"
                    .split(' ')
                    .map(word)
                    .collect(),
                context_names: vec![],
                context_emails: vec![],
                context_text: kontext.to_string(),
                generated_title: String::new(),
            },
            session("b", "dann hat Grandfall geantwortet und", &[]),
        ];
        let found = scan(
            &sessions,
            &["Grandpfeil => Grandfall".into()],
            &[],
            &Options::default(),
        );
        assert!(
            !found
                .iter()
                .any(|vorschlag| vorschlag.alias == "Grandpfeil"),
            "richtige Schreibweise als Verhoerung vorgeschlagen (Kontext {kontext:?}): {:?}",
            pairs(&found)
        );
    }
}

/// Die Invariante aus dem Test darueber, breit abgesucht.
///
/// Erschoepfend ueber alle Anordnungen aus zwei Schreibweisen, drei
/// Kontextbelegungen und beiden Woerterbuch-Richtungen: KEIN Lauf schlaegt je
/// eine richtige Schreibweise als Verhoerung vor. Das ist der empirische Teil
/// der Antwort auf die Frage, ob die geloeschte Zeile lebendig war.
#[test]
fn keine_anordnung_dreht_ein_bekanntes_paar_um() {
    let optionen = Options::default();
    let mut laeufe = 0;

    for eintrag in [
        "Grandpfeil => Grandfall",
        "Grandfall => Grandpfeil",
        "Grandpfeil",
        "Grandfall",
    ] {
        for kontext_a in ["", "Grandfall Strategie", "Grandpfeil Strategie"] {
            for kontext_b in ["", "Grandfall Strategie", "Grandpfeil Strategie"] {
                for (erst, dann) in [("Grandpfeil", "Grandfall"), ("Grandfall", "Grandpfeil")] {
                    let sessions = [
                        ScanSession {
                            session_id: "a".into(),
                            words: format!(" wir haben {erst} gefragt und")
                                .split(' ')
                                .map(word)
                                .collect(),
                            context_names: vec![],
                            context_emails: vec![],
                            context_text: kontext_a.to_string(),
                            generated_title: String::new(),
                        },
                        ScanSession {
                            session_id: "b".into(),
                            words: format!(" dann hat {dann} geantwortet und")
                                .split(' ')
                                .map(word)
                                .collect(),
                            context_names: vec![],
                            context_emails: vec![],
                            context_text: kontext_b.to_string(),
                            generated_title: String::new(),
                        },
                    ];
                    let found = scan(&sessions, &[eintrag.to_string()], &[], &optionen);
                    laeufe += 1;

                    let kanon = crate::Vocabulary::parse([eintrag]).canonical_terms();
                    for vorschlag in &found {
                        assert!(
                            !kanon
                                .iter()
                                .any(|richtig| richtig.to_lowercase()
                                    == vorschlag.alias.to_lowercase()),
                            "Umkehrung bei Eintrag {eintrag:?}, Kontext {kontext_a:?}/{kontext_b:?}: {:?}",
                            pairs(&found)
                        );
                    }
                }
            }
        }
    }

    assert_eq!(
        laeufe,
        4 * 3 * 3 * 2,
        "die Absuche hat nicht alles abgedeckt"
    );
}

/// Ein bekanntes Paar bleibt gesperrt, auch wenn die Verhoerung selbst als
/// belegt gilt.
///
/// Der reale Weg dahin: Sitzungstitel werden von einem Modell erzeugt. Trifft
/// ein solcher Titel die Verhoerung ("Grandfall Strategie"), steht sie in der
/// Anker-Menge -- und weil Netz 1 die alphabetisch erste belegte Schreibweise
/// waehlt, wuerde daraus der Vorschlag "Grandpfeil => Grandfall" UMGEDREHT:
/// kuenftig "Grandpfeil" durch "Grandfall" ersetzen. Genau die Entscheidung,
/// die der Betreiber schon getroffen hat, nur andersherum.
#[test]
fn ein_bekanntes_paar_bleibt_auch_umgedreht_gesperrt() {
    let sessions = [
        session_mit_text(
            "a",
            "wir haben Grandpfeil gefragt und",
            "Grandfall Strategie",
        ),
        session_mit_text("b", "dann hat Grandfall geantwortet und", ""),
    ];
    let found = scan(
        &sessions,
        &["Grandpfeil => Grandfall".into()],
        &[],
        &Options::default(),
    );
    assert!(
        found.is_empty(),
        "bekanntes Paar umgedreht vorgeschlagen: {:?}",
        pairs(&found)
    );
}

/// Zwei Laeufe ueber dieselben Daten liefern dieselbe Liste in derselben
/// Reihenfolge -- sonst waere die Oberflaeche jedes Mal anders sortiert.
#[test]
fn der_lauf_ist_wiederholbar() {
    let sessions = [
        session("a", "hier stand Meiners und dort Grandpfeil dazu", &[]),
        session("b", "dort stand Meinerz und hier Grandfall dazu", &[]),
    ];
    let liste = ["Grandpfeil".to_string(), "Meiners".to_string()];
    let options = Options::default();
    let first = scan(&sessions, &liste, &[], &options);
    let second = scan(&sessions, &liste, &[], &options);
    assert_eq!(pairs(&first), pairs(&second));
    assert!(first.len() >= 2);
}

