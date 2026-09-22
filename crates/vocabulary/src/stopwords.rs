//! Waechterliste gewoehnlicher deutscher Woerter.
//!
//! Der Grund steht in einem Satz: die Koelner Phonetik kodiert "Vurden" und
//! "werden" identisch. Ohne diese Liste wuerde die Klangstufe jedes "werden" in
//! des Betreibers Transkript in den Ortsnamen "Vurden" verwandeln -- und "werden" ist
//! eines der haeufigsten Woerter der deutschen Sprache.
//!
//! Am Satzanfang steht ein solches Wort gross geschrieben ("Werden wir das
//! schaffen?"), also faengt die Grossschreibungs-Regel es NICHT. Diese Liste
//! ist der Grund, warum die Klangstufe ueberhaupt eingeschaltet bleiben kann.
//!
//! Die Liste hat seit dem 04.09.2026 ZWEI Teile, und der Unterschied ist
//! wichtig:
//!
//! 1. [`COMMON_GERMAN_WORDS`] -- von Hand gepflegt, jeder Eintrag stammt aus
//!    einem gemessenen Fehlgriff. Hier stehen Funktionswoerter, die
//!    haeufigsten Verben in ihren gaengigen Beugungen, Zahlwoerter,
//!    Gespraechspartikeln und die englischen Alltagswoerter, die in deutscher
//!    Bueroerede vorkommen.
//! 2. [`WORTLISTE_DE`] -- 38.424 gewoehnliche deutsche Wortformen, aus dem
//!    Baumbank-Korpus UD German-GSD gewonnen (CC BY-SA 4.0, Herkunft und
//!    Bedingungen in `ATTRIBUTIONS.md`). Sie liegt als Datei daneben und wird
//!    beim Uebersetzen fest ins Binary gelegt.
//!
//! WARUM der zweite Teil dazukam (Befund des Betreibers, 04.09.2026): die
//! Handliste umfasste 988 Eintraege und deckte Funktionswoerter gut ab,
//! INHALTSWOERTER aber nicht. Der Vorschlags-Lauf hielt deshalb "Medien",
//! "Salz", "Weiteres" und "Messen" fuer verdaechtig und legte Paare wie
//! `Messen <- Medien` und `Sales <- Salz` vor -- zwei gewoehnliche Woerter
//! gegeneinander getauscht. Ein Postfach, in dem jeder zweite Eintrag so
//! aussieht, liest niemand.
//!
//! WARUM AUSGERECHNET diese Quelle, und das ist der Kern: eine Wortliste, die
//! EIGENNAMEN enthaelt, macht den Lauf schlechter statt besser. Steht
//! "Tofmann" darin, stirbt der belegte Vorschlag `Tofmann <- Hochmann`
//! stillschweigend. GEMESSEN am 04.09.2026 an drei Kandidaten:
//!
//! - Hunspell `de_DE_frami`: enthaelt acht der geprueften Vor-, Nach- und
//!   Ortsnamen, darunter "Simon" und "Silicon" -- haette sechs belegte
//!   Vorschlaege getoetet. Ausserdem GPLv2/GPLv3, also fuer diese App
//!   ohnehin gesperrt.
//! - Haeufigkeitsliste aus Untertiteln (`de_50k`): enthaelt zusaetzlich vier
//!   weitere Namen aus derselben Pruefung. Noch schlechter.
//! - UD German-GSD mit Wortartfilter: die Baumbank markiert Eigennamen als
//!   `PROPN`. Wer sie herausnimmt, bekommt Wortformen ohne Namen. Von 51
//!   geprueften Gefahrwoertern blieben genau ZWEI uebrig ("malte" als
//!   Vergangenheitsform von "malen", "landwehr" als historischer Begriff).
//!
//! Der Preis dieser zwei ist bekannt und angenommen: ein Name, der zugleich
//! ein gewoehnliches deutsches Wort ist, bekommt keinen Vorschlag mehr. Das
//! ist die billige Fehlerrichtung -- ein fehlender Vorschlag kostet einen
//! Eintrag, ein zerstoertes Alltagswort kostet Vertrauen ins Transkript.
//!
//! Ein Wort steht in der Datei, wenn es im Korpus OEFTER als gewoehnliches
//! Wort denn als Eigenname vorkommt. Reine Gleichheit ("nie als Eigenname
//! getaggt") waere zu streng: sie haette "Medien" (14x Nomen, 2x Eigenname)
//! und "Messen" (21x zu 1x) herausgeworfen -- also genau die beiden Woerter,
//! wegen denen die Liste ueberhaupt gebaut wurde.
//!
//! Die Handliste bleibt daneben und bleibt der Ort fuer Nachtraege: die
//! Baumbank kennt kein "klick ab" und kein "Kling", und die Messung am echten
//! Material ist weiterhin das, was entscheidet.
//!
//! Seit dem 03.09.2026 ist diese Liste die EINZIGE. Das Frontend hatte eine
//! eigene Kopie samt eigener Normalisierung; sie ist geloescht. Die
//! Einstellungen fragen jetzt ueber den Befehl `vocabulary_risky_aliases`
//! (plugins/local-stt) genau die Entscheidung ab, die hier faellt -- wer hier
//! ein Wort nachtraegt oder an `normalize` dreht, aendert damit auch, was der
//! Mensch als "wird ignoriert" angezeigt bekommt.

/// Kleingeschrieben, sortiert nach Sachgruppen, damit Nachtragen leicht faellt.
///
/// `rustfmt::skip`, damit die Sachgruppen samt ihren Kommentaren erhalten
/// bleiben -- ein Wort je Zeile macht aus 100 Zeilen 900 und aus der Gliederung
/// eine Liste.
#[rustfmt::skip]
const COMMON_GERMAN_WORDS: &[&str] = &[
    // Artikel, Pronomen, Praepositionen, Konjunktionen
    "der", "die", "das", "den", "dem", "des", "ein", "eine", "einen", "einem", "einer", "eines",
    "kein", "keine", "keinen", "keinem", "keiner", "keines", "ich", "du", "er", "sie", "es",
    "wir", "ihr", "mich", "dich", "sich", "uns", "euch", "mir", "dir", "ihm", "ihn", "ihnen",
    "mein", "dein", "sein", "unser", "euer", "man", "wer", "wen", "wem", "was", "welche",
    "welcher", "welches", "dieser", "diese", "dieses", "diesen", "diesem", "jener", "jene",
    "jenes", "alle", "alles", "allem", "allen", "jeder", "jede", "jedes", "manche", "einige",
    "andere", "anderen", "beide", "beiden", "selbst", "selber", "und", "oder", "aber", "denn",
    "sondern", "doch", "weil", "dass", "ob", "wenn", "als", "wie", "damit", "sodass", "obwohl",
    "waehrend", "bevor", "nachdem", "seit", "seitdem", "bis", "sobald", "falls", "in", "im",
    "an", "am", "auf", "aus", "bei", "beim", "mit", "nach", "von", "vom", "vor", "zu", "zum",
    "zur", "ueber", "unter", "durch", "gegen", "ohne", "um", "fuer", "hinter", "neben",
    "zwischen", "trotz", "wegen", "statt", "innerhalb", "ausserhalb", "gegenueber", "entlang",
    "je", "pro",
    // Adverbien und Partikeln
    "nicht", "nur", "auch", "noch", "schon", "sehr", "mehr", "weniger", "viel", "viele",
    "wenig", "etwas", "nichts", "immer", "nie", "niemals", "oft", "manchmal", "selten",
    "wieder", "erst", "gerade", "eben", "gleich", "bald", "spaeter", "frueher", "heute",
    "gestern", "morgen", "jetzt", "dann", "damals", "hier", "dort", "da", "wo", "woher",
    "wohin", "warum", "weshalb", "wieso", "wann", "sehr", "ganz", "fast", "kaum", "genau",
    "eigentlich", "vielleicht", "wahrscheinlich", "sicher", "natuerlich", "leider",
    "hoffentlich", "wirklich", "wohl", "halt", "mal", "eben", "einfach", "sogar", "besonders",
    "zumindest", "wenigstens", "trotzdem", "deshalb", "deswegen", "daher", "also", "somit",
    "zwar", "jedoch", "allerdings", "ausserdem", "zudem", "ebenfalls", "ebenso", "genauso",
    "anders", "zusammen", "allein", "wieder", "zurueck", "weiter", "voran", "hinein", "heraus",
    "hinaus", "herein", "hinauf", "herunter", "vorbei", "entlang",
    // Die haeufigsten Verben, in den Formen, die wirklich vorkommen
    "sein", "bin", "bist", "ist", "sind", "seid", "war", "warst", "waren", "wart", "waere",
    "waeren", "gewesen", "haben", "habe", "hast", "hat", "habt", "hatte", "hattest", "hatten",
    "haette", "haetten", "gehabt", "werden", "werde", "wirst", "wird", "werdet", "wurde",
    "wurdest", "wurden", "wurdet", "worden", "wuerde", "wuerden", "koennen", "kann", "kannst",
    "koennt", "konnte", "konnten", "koennte", "koennten", "gekonnt", "muessen", "muss", "musst",
    "muesst", "musste", "mussten", "muesste", "muessten", "gemusst", "sollen", "soll", "sollst",
    "sollt", "sollte", "sollten", "gesollt", "wollen", "will", "willst", "wollt", "wollte",
    "wollten", "gewollt", "duerfen", "darf", "darfst", "duerft", "durfte", "durften", "duerfte",
    "duerften", "moegen", "mag", "magst", "moegt", "mochte", "mochten", "moechte", "moechten",
    "machen", "mache", "machst", "macht", "machte", "machten", "gemacht", "gehen", "gehe",
    "gehst", "geht", "ging", "gingen", "gegangen", "kommen", "komme", "kommst", "kommt", "kam",
    "kamen", "gekommen", "sagen", "sage", "sagst", "sagt", "sagte", "sagten", "gesagt", "sehen",
    "sehe", "siehst", "sieht", "sah", "sahen", "gesehen", "geben", "gebe", "gibst", "gibt",
    "gebt", "gab", "gaben", "gegeben", "nehmen", "nehme", "nimmst", "nimmt", "nahm", "nahmen",
    "genommen", "finden", "finde", "findest", "findet", "fand", "fanden", "gefunden", "stehen",
    "stehe", "stehst", "steht", "stand", "standen", "gestanden", "bleiben", "bleibe", "bleibst",
    "bleibt", "blieb", "blieben", "geblieben", "liegen", "liege", "liegst", "liegt", "lag",
    "lagen", "gelegen", "halten", "halte", "haeltst", "haelt", "hielt", "hielten", "gehalten",
    "lassen", "lasse", "laesst", "liess", "liessen", "gelassen", "setzen", "setze", "setzt",
    "setzte", "setzten", "gesetzt", "stellen", "stelle", "stellst", "stellt", "stellte",
    "stellten", "gestellt", "denken", "denke", "denkst", "denkt", "dachte", "dachten",
    "gedacht", "wissen", "weiss", "weisst", "wisst", "wusste", "wussten", "gewusst", "glauben",
    "glaube", "glaubst", "glaubt", "glaubte", "glaubten", "geglaubt", "meinen", "meine",
    "meinst", "meint", "meinte", "meinten", "gemeint", "brauchen", "brauche", "brauchst",
    "braucht", "brauchte", "brauchten", "gebraucht", "arbeiten", "arbeite", "arbeitest",
    "arbeitet", "arbeitete", "gearbeitet", "sprechen", "spreche", "sprichst", "spricht",
    "sprach", "sprachen", "gesprochen", "reden", "rede", "redest", "redet", "redete", "geredet",
    "fragen", "frage", "fragst", "fragt", "fragte", "fragten", "gefragt", "schauen", "schaue",
    "schaust", "schaut", "schaute", "geschaut", "klingen", "klinge", "klingst", "klingt",
    "klang", "klangen", "geklungen", "bringen", "bringe", "bringst", "bringt", "brachte",
    "brachten", "gebracht", "laufen", "laufe", "laeuft", "lief", "liefen", "gelaufen", "fahren",
    "fahre", "faehrst", "faehrt", "fuhr", "fuhren", "gefahren", "schreiben", "schreibe",
    "schreibst", "schreibt", "schrieb", "schrieben", "geschrieben", "lesen", "lese", "liest",
    "las", "lasen", "gelesen", "heissen", "heisse", "heisst", "hiess", "hiessen", "geheissen",
    "zeigen", "zeige", "zeigst", "zeigt", "zeigte", "gezeigt", "beginnen", "beginne", "beginnt",
    "begann", "begannen", "begonnen", "versuchen", "versuche", "versuchst", "versucht",
    "versuchte", "bauen", "baue", "baust", "baut", "baute", "gebaut", "merken", "merke",
    "merkst", "merkt", "merkte", "gemerkt", "warten", "warte", "wartest", "wartet", "wartete",
    "gewartet", "kennen", "kenne", "kennst", "kennt", "kannte", "kannten", "gekannt", "helfen",
    "helfe", "hilfst", "hilft", "half", "halfen", "geholfen", "hoeren", "hoere", "hoerst",
    "hoert", "hoerte", "gehoert", "passen", "passe", "passt", "passte", "gepasst", "packen",
    "packe", "packst", "packt", "packte", "gepackt", "schicken", "schicke", "schickst",
    "schickt", "schickte", "geschickt", "legen", "lege", "legst", "legt", "legte", "gelegt",
    "ziehen", "ziehe", "ziehst", "zieht", "zog", "zogen", "gezogen", "fallen", "faellt", "fiel",
    "fielen", "gefallen", "tun", "tue", "tust", "tut", "tat", "taten", "getan",
    // Haeufige Nomen aus Buero- und Projektalltag
    "sache", "sachen", "ding", "dinge", "zeit", "zeiten", "jahr", "jahre", "jahren", "monat",
    "monate", "woche", "wochen", "tag", "tage", "tagen", "stunde", "stunden", "minute",
    "minuten", "mensch", "menschen", "leute", "frau", "frauen", "mann", "maenner", "kind",
    "kinder", "kunde", "kunden", "team", "teams", "firma", "firmen", "seite", "seiten", "frage",
    "fragen", "antwort", "antworten", "problem", "probleme", "loesung", "loesungen", "arbeit",
    "arbeiten", "projekt", "projekte", "termin", "termine", "mail", "mails", "punkt", "punkte",
    "teil", "teile", "art", "weise", "grund", "gruende", "beispiel", "beispiele", "moment",
    "momente", "stelle", "stellen", "platz", "raum", "haus", "hause", "hand", "haende", "kopf",
    "auge", "augen", "wort", "worte", "woerter", "name", "namen", "nummer", "nummern", "geld",
    "preis", "preise", "kosten", "wert", "werte", "ende", "anfang", "beginn", "schluss", "ziel",
    "ziele", "plan", "plaene", "idee", "ideen", "text", "texte", "bild", "bilder", "datei",
    "dateien", "seite", "buch", "buecher", "welt", "land", "stadt", "weg", "wege", "fall",
    "faelle", "form", "formen", "system", "systeme", "prozess", "prozesse", "schritt",
    "schritte", "version", "versionen",
    // Zahlwoerter
    "null", "eins", "zwei", "drei", "vier", "fuenf", "sechs", "sieben", "acht", "neun", "zehn",
    "elf", "zwoelf", "zwanzig", "dreissig", "vierzig", "fuenfzig", "hundert", "tausend",
    "erste", "erster", "erstes", "zweite", "zweiter", "dritte", "letzte", "letzten", "naechste",
    "naechsten",
    // Haeufige Adjektive
    "gut", "gute", "guten", "guter", "gutes", "besser", "beste", "besten", "schlecht",
    "schlechte", "gross", "grosse", "grossen", "klein", "kleine", "kleinen", "neu", "neue",
    "neuen", "alt", "alte", "alten", "lang", "lange", "langen", "kurz", "kurze", "hoch", "hohe",
    "niedrig", "schnell", "schnelle", "langsam", "richtig", "richtige", "falsch", "falsche",
    "wichtig", "wichtige", "moeglich", "moegliche", "noetig", "einfach", "einfache", "schwer",
    "schwierig", "klar", "klare", "fertig", "offen", "offene", "voll", "leer", "ganze",
    "ganzen", "halbe", "eigene", "eigenen", "echte", "echten", "gleiche", "gleichen",
    "naechste", "weitere", "weiteren", "bestimmte", "gesamte", "einzelne", "verschiedene",
    "meiste", "meisten",
    // Gespraechspartikeln, die in Transkripten haeufig sind
    "ja", "nein", "ok", "okay", "genau", "gut", "also", "so", "naja", "aeh", "aehm", "hm",
    "tja", "danke", "bitte", "hallo", "tschuess", "moin", "servus", "gerne", "klar",
    // Nachgetragen 02.09.2026 aus zwei Saetzen, an denen die ALIAS-Stufe
    // des Betreibers eigene Liste gegen ihn gewendet haette: "Es macht Kling und die
    // Tuer geht auf." (Alias "Kling" fuer "Flinck") und "Klick ab und schliess
    // das Fenster." (Alias "Klick ab" fuer "ClickUp"). "ab" fehlte schlicht --
    // die Liste kannte "an", "auf" und "aus", aber nicht die vierte dieser
    // Gruppe.
    "ab", "kling", "klingel", "klick", "klicks", "klicke", "klickst", "klickt", "klicken",
    "geklickt", "tuer", "tueren", "fenster", "schliessen", "schliesst", "schliess",
    // Am 02.09.2026 GEMESSEN, nicht vermutet: diese Woerter hat der Nachlauf
    // in 390 echten Transkripten (1.886.759 Woerter) tatsaechlich zerstoert,
    // bevor die Klangstufen abgeschaltet wurden. Sie stehen hier, weil das der
    // dokumentierte Weg ist -- aber sie sind KEIN Beleg dafuer, dass die Liste
    // jetzt reicht. Sie reicht nicht: siehe die Begruendung an
    // `Options::default`.
    "werten", "wert", "werte", "werts", "lieferant", "lieferanten", "clinch", "klinik",
    "kliniken", "klinke", "klinken", "linie", "linien",
    // Am 03.09.2026 GEMESSEN, beim ersten Lauf der beiden Vorschlags-Netze
    // (`proposals`) ueber des Betreibers neun lebende Transkripte: das hier sind die
    // Woerter, die als angebliche VERHOERUNG vorgeschlagen wurden, obwohl sie
    // gewoehnliches Deutsch sind -- "Rechner" sollte zu "Retainer" werden,
    // "Webseite" zu "Website", "Vielen" (aus "Vielen Dank") zu "Vilken".
    // Jedes einzelne stammt aus einem echten Vorschlag, keines ist geraten;
    // die Wortfamilien daneben sind mitgenommen, weil der naechste Lauf sonst
    // die Mehrzahl desselben Wortes vorschlaegt.
    //
    // Der Nutzen geht ueber die Netze hinaus: dieselbe Liste haelt die
    // Klangstufen des Nachlaufs davon ab, diese Woerter zu ERSETZEN.
    //
    // RAUS und WIEDER REIN am 03.09.2026. Der Zwischenstand nahm "website"
    // und "websites" heraus mit der Begruendung, diese Liste heisse
    // "gewoehnliche deutsche Woerter" und englische Lehnwoerter gehoerten
    // nicht hinein. Die Begruendung faellt an der Liste selbst: "app",
    // "apps", "link", "links" und "info" stehen zwei Zeilen weiter unten und
    // sind genauso englisch. Das Merkmal ist nicht die Herkunft eines Wortes,
    // sondern ob es in gewoehnlicher deutscher Rede vorkommt -- und das tut
    // "Website" in jedem Geschaeftsgespraech.
    //
    // Gemessen wurde die Herausnahme ausserdem an genau EINEM der vier
    // Verbraucher dieser Liste (den beiden Netzen). Die anderen drei sitzen
    // im `matcher` und in `Vocabulary::risky_aliases`; dort entscheidet
    // dieselbe Liste, ob eine eingetragene Verhoerung ueberhaupt angewandt
    // wird. Ohne "website" nimmt die Alias-Stufe einen Eintrag
    // `Webseite => Website` an und ersetzt danach in JEDEM kuenftigen
    // Gespraech jedes richtig geschriebene "Website".
    //
    // "story" und "word" bleiben draussen: "Word" ist ein Produktname, den
    // ein Mensch zu Recht ins Woerterbuch eintragen koennte, und die Richtung,
    // die dort gefaehrlich war ("Word <- Wort"), haelt bereits "wort" /
    // "worte" / "woerter" weiter oben.
    "website", "websites",
    "rechner", "rechnern", "rechnung", "rechnungen", "webseite", "webseiten",
    "haelfte", "haelften", "hilfe", "hilfen", "jahr", "jahre", "jahres", "jahren",
    "junge", "jungen", "hunger", "kalender", "kalendern", "link", "links", "verein", "vereine",
    "vereinen", "versehen", "klasse", "klassen", "melde", "meldet", "melden", "meldung",
    "meldungen", "umsatz", "umsaetze", "ansatz", "ansaetze", "aufgabe", "aufgaben", "ausgabe",
    "ausgaben", "angebot", "angebote", "angeboten", "agentur", "agenturen", "gruppe", "gruppen",
    "position", "positionen", "impuls", "impulse", "dimension", "dimensionen", "konflikt",
    "konflikte", "unternehmen", "unternehmer", "beratung", "beratungen", "betrachtung",
    "ueberlegung", "ueberlegungen", "ueberleitung", "veraenderung", "veraenderungen",
    "vertrauen", "vertrieb", "vorlage", "vorlagen", "variante", "varianten", "urlaub",
    "urlaubs", "sorge", "sorgen", "kachel", "kacheln", "kamera", "kameras", "karte", "karten",
    "eltern", "alter", "ebene", "ebenen", "agenda", "einstieg", "vorwege", "verzeihung",
    "verzahnung", "supervision", "telefon", "telefone", "themen", "thema", "teilen", "start",
    "strudel", "futter", "wand", "waende", "info", "infos", "gespraech",
    "gespraeche", "gespraechen", "gespraechs", "viel", "viele", "vielen", "vieles", "zucker",
    "app", "apps",
    // Am 04.09.2026 GEMESSEN am selben Korpus, nach dem Einzug der grossen
    // Wortliste. Die Baumbank ist deutsch und kennt diese Woerter nicht --
    // in deutscher Bueroerede kommen sie trotzdem vor, und genau das ist das
    // Merkmal dieser Liste (siehe die Begruendung zu "website" weiter oben).
    //
    // Die ersten drei stammen aus echten Fehlvorschlaegen des Betreibers:
    // "Manuals" sollte zu einem klanggleichen Vornamen werden, "Salz" zu "Sales",
    // "Features" zu "Weiteres". Der Rest sind die uebrigen englischen
    // Alltagswoerter aus demselben Zensus der 1.006 verbliebenen Kandidaten;
    // sie sind mitgenommen, weil sonst jedes einzelne beim naechsten Lauf
    // seinen eigenen Fehlvorschlag erzeugt.
    //
    // NICHT dabei und mit Absicht: Ortsnamen (Hamburg, Bremen, Leipzig) und
    // Vornamen. Die sollen vorschlagbar BLEIBEN -- sie sind der Regelfall
    // einer echten Verhoerung.
    "manual", "manuals", "sale", "sales", "feature", "features",
    "tool", "tools", "case", "cases", "ticket", "tickets", "footer", "header",
    "cloud", "source", "sources", "newsletter", "newsletters", "learning",
    "learnings", "landingpage", "landingpages", "windows", "open", "first",
    "yeah", "connect", "content", "template", "templates", "update", "updates",
    "review", "reviews", "feedback", "workshop", "workshops", "meeting",
    "meetings", "call", "calls", "slide", "slides", "task", "tasks", "account",
    "accounts", "budget", "budgets", "deadline", "deadlines", "briefing",
    "briefings", "shop", "shops", "screen", "screens", "layout", "layouts",
];

/// Wahr, wenn `word` ein gewoehnliches deutsches Wort ist, das die Klangstufe
/// nicht anfassen darf.
///
/// Vergleich ohne Ruecksicht auf Gross-/Kleinschreibung, weil genau der
/// Satzanfang der gefaehrliche Fall ist. Umlaute werden auf ihre
/// ae/oe/ue-Schreibweise gebracht, damit "Waeren" und "Wären" beide treffen.
pub fn is_common_german_word(word: &str) -> bool {
    let normalized = normalize(word);
    if normalized.is_empty() {
        return false;
    }
    lookup().contains(normalized.as_str())
}

/// Wahr, wenn `phrase` AUSSCHLIESSLICH aus gewoehnlichen deutschen Woertern
/// besteht -- die Bedingung, unter der eine eingetragene Verhoerung von
/// gewoehnlicher Sprache nicht mehr zu unterscheiden ist.
///
/// Warum "alle" und nicht "eines": des Betreibers echte Liste enthaelt `Seda das` als
/// Verhoerung von "Sedacz". Dort ist `das` ein gewoehnliches Wort, `Seda`
/// aber nicht -- die Wortfolge als ganze kommt in deutscher Rede nicht vor, die
/// Ersetzung ist sicher. `Klick ab` und `Kling` sind der andere Fall: JEDES
/// ihrer Woerter ist gewoehnlich, und damit ist die Folge selbst gewoehnliche
/// Sprache. Ein "eines genuegt" haette `Seda das` mit erschlagen.
///
/// Die Klangstufen benutzen weiterhin die strengere "eines genuegt"-Form: die
/// raten, diese Pruefung urteilt ueber etwas, das ein Mensch getippt hat.
pub fn is_all_common_german_phrase(phrase: &str) -> bool {
    let mut any = false;
    for word in phrase.split_whitespace() {
        if normalize(word).is_empty() {
            continue;
        }
        any = true;
        if !is_common_german_word(word) {
            return false;
        }
    }
    any
}

/// Gewoehnliche deutsche Wortformen aus UD German-GSD, ein Wort je Zeile,
/// bereits in der `normalize`-Schreibweise (klein, Umlaute aufgeloest).
///
/// Fest ins Binary gelegt und nicht zur Laufzeit geladen: der Waechter
/// entscheidet, ob eine Ersetzung stattfindet, und darf nie an einer Datei
/// haengen, die auf einer fremden Kiste fehlt. Ohne Liste waere er still
/// schwaecher -- genau die Sorte Ausfall, die wie ein sauberes Ergebnis
/// aussieht.
///
/// Die Datei traegt KEINE Kommentarzeilen, damit `lines()` genuegt und kein
/// Zerleger dazwischen steht, der eines Tages still das Falsche tut.
const WORTLISTE_DE: &str = include_str!("../wortliste-de.txt");

/// Einmal gebaut, danach nachgeschlagen -- die Liste wird pro Wort des
/// Transkripts befragt, ein linearer Durchlauf waere Verschwendung.
fn lookup() -> &'static std::collections::HashSet<&'static str> {
    static LOOKUP: std::sync::OnceLock<std::collections::HashSet<&'static str>> =
        std::sync::OnceLock::new();
    LOOKUP.get_or_init(|| COMMON_GERMAN_WORDS.iter().copied().collect())
}

/// Wahr, wenn `word` ein gewoehnliches Wort der deutschen Alltagssprache ist --
/// die WEITE Pruefung, Handliste plus [`WORTLISTE_DE`].
///
/// Der Unterschied zu [`is_common_german_word`] ist keine Feinheit, sondern der
/// Kern der Aenderung vom 04.09.2026, und er laeuft entlang der Frage, WER die
/// Rechnung zahlt:
///
/// - [`is_common_german_word`] entscheidet, ob eine von einem MENSCHEN
///   getippte Verhoerung angewandt wird. Ein Fehlalarm dort heisst: der
///   Betreiber traegt einen Eintrag ein, und die App ignoriert ihn stumm. Diese
///   Pruefung bleibt deshalb bei der Handliste, in der jedes Wort einen
///   gemessenen Anlass hat.
/// - Diese hier entscheidet, ob eine MASCHINE einen Vorschlag machen darf. Ein
///   Fehlalarm kostet einen Vorschlag, der ausbleibt -- die billige Richtung.
///
/// GEMESSEN, warum die Trennung sein muss und nicht Zierde ist: mit der grossen
/// Liste hinter der Wortfolgen-Pruefung gilt "Nord Werk" als gewoehnliche
/// Sprache, weil "Nord" und "Werk" beide fuer sich deutsche Woerter sind. Ein
/// zweiwortiger Eintrag des Betreibers -- die Verhoerung eines
/// zusammengesetzten Namens, also der Regelfall -- waere damit tot gewesen. Ein
/// Test hat es beim ersten Uebersetzen gefangen.
pub fn is_ordinary_german_word(word: &str) -> bool {
    let normalized = normalize(word);
    if normalized.is_empty() {
        return false;
    }
    if lookup().contains(normalized.as_str()) {
        return true;
    }
    wide_lookup().contains(normalized.as_str())
}

/// Der zweite Satz, getrennt gehalten: so ist an jeder Aufrufstelle sichtbar,
/// welche der beiden Fragen gestellt wird.
fn wide_lookup() -> &'static std::collections::HashSet<&'static str> {
    static LOOKUP: std::sync::OnceLock<std::collections::HashSet<&'static str>> =
        std::sync::OnceLock::new();
    LOOKUP.get_or_init(|| {
        WORTLISTE_DE
            .lines()
            .map(str::trim)
            .filter(|zeile| !zeile.is_empty())
            .collect()
    })
}

fn normalize(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    for character in word.chars() {
        match character {
            'ä' | 'Ä' => out.push_str("ae"),
            'ö' | 'Ö' => out.push_str("oe"),
            'ü' | 'Ü' => out.push_str("ue"),
            'ß' => out.push_str("ss"),
            _ if character.is_alphabetic() => {
                out.extend(character.to_lowercase());
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn die_kollision_die_diese_liste_rechtfertigt() {
        // "werden" hat denselben Koelner-Code wie der Ortsname "Vurden".
        assert_eq!(
            crate::koelner::encode("werden"),
            crate::koelner::encode("Vurden")
        );
        // Nur diese Liste haelt die Ersetzung auf.
        assert!(is_common_german_word("werden"));
        assert!(!is_common_german_word("Vurden"));
    }

    #[test]
    fn der_satzanfang_ist_der_gefaehrliche_fall() {
        // Gross geschrieben, weil Satzanfang -- die Grossschreibungs-Regel
        // greift hier nicht, diese Liste schon.
        assert!(is_common_german_word("Werden"));
        assert!(is_common_german_word("Klingt"));
        assert!(is_common_german_word("Kommt"));
    }

    #[test]
    fn umlaute_treffen_in_beiden_schreibweisen() {
        assert!(is_common_german_word("wären"));
        assert!(is_common_german_word("waeren"));
        assert!(is_common_german_word("müssen"));
        assert!(is_common_german_word("MÜSSEN"));
    }

    #[test]
    fn satzzeichen_stoeren_nicht() {
        assert!(is_common_german_word("werden,"));
        assert!(is_common_german_word("genau."));
    }

    #[test]
    fn eigennamen_stehen_nicht_drin() {
        for name in [
            "Ohlandez",
            "Grandpfeil",
            "Talwiese",
            "Flinck",
            "Merkentin",
            "Nordwerk",
            "Netzflug",
            "Buchwerk",
            "Bringado",
            "Tarnow",
            "Tofmann",
        ] {
            assert!(
                !is_common_german_word(name),
                "{name} darf nicht gesperrt sein"
            );
        }
    }

    /// Die beiden Saetze aus des Betreibers eigener Liste, an denen die Alias-Stufe
    /// am 02.09.2026 gewoehnliche Sprache zerstoert haette.
    #[test]
    fn die_beiden_gefaehrlichen_aliasse_sind_gewoehnliche_sprache() {
        assert!(is_all_common_german_phrase("Kling"));
        assert!(is_all_common_german_phrase("Klick ab"));
        assert!(is_all_common_german_phrase("klick ab"));
    }

    /// Die Gegenprobe, die "eines genuegt" nicht bestanden haette: `Seda das`
    /// enthaelt ein gewoehnliches Wort, ist als Folge aber keines.
    #[test]
    fn eine_folge_mit_einem_namen_darin_ist_keine_gewoehnliche_sprache() {
        for alias in [
            "Seda das",
            "Sarnec",
            "Phono Werk",
            "NOR Druck Technik",
            "Nord Werk",
            "Glink",
            "Tof Werk",
            "Tochmann",
        ] {
            assert!(
                !is_all_common_german_phrase(alias),
                "{alias} darf nicht als gewoehnliche Sprache gelten"
            );
        }
    }

    /// Das Merkmal ist der GEBRAUCH, nicht die Herkunft.
    ///
    /// Am 03.09.2026 flogen "website" und "websites" mit der Begruendung
    /// heraus, englische Lehnwoerter gehoerten nicht in eine Liste deutscher
    /// Woerter. Die uebrigen Eintraege dieser Zeile widerlegen das: sie stehen
    /// seit derselben Runde drin und sind genauso englisch. Was zaehlt, ist
    /// ob ein Wort in gewoehnlicher deutscher Rede vorkommt -- denn nur
    /// darueber entscheidet der Waechter im `matcher`, der eine eingetragene
    /// Verhoerung anwendet oder eben nicht.
    ///
    /// Ohne "website" nimmt die Alias-Stufe einen Eintrag
    /// `Webseite => Website` an und ersetzt danach jedes richtig geschriebene
    /// "Website" in jedem kuenftigen Gespraech.
    #[test]
    fn ein_englisches_alltagswort_gilt_als_gewoehnliche_sprache() {
        for wort in ["Website", "websites", "App", "Apps", "Link", "Info"] {
            assert!(
                is_common_german_word(wort),
                "{wort} kommt in gewoehnlicher deutscher Rede vor"
            );
        }
    }

    #[test]
    fn eine_leere_folge_ist_keine_gewoehnliche_sprache() {
        assert!(!is_all_common_german_phrase(""));
        assert!(!is_all_common_german_phrase("   "));
        assert!(!is_all_common_german_phrase("--- 42"));
    }

    /// Der Grund, aus dem die grosse Liste ueberhaupt dazugekommen ist.
    ///
    /// Diese vier Woerter haben am 04.09.2026 Fehlvorschlaege erzeugt --
    /// "Messen" gegen "Medien", "Sales" gegen "Salz", "Weiteres" gegen
    /// "Features". Die Handliste kannte keines davon; sie deckt
    /// Funktionswoerter ab, nicht Inhaltswoerter.
    ///
    /// Wird `include_str!` entfernt oder die Datei geleert, ist diese Zeile
    /// rot. Sie faellt NICHT auf einen Handlisten-Treffer zurueck -- die
    /// zweite Haelfte haelt fest, dass die Woerter dort wirklich fehlen.
    #[test]
    fn die_grosse_wortliste_traegt_die_inhaltswoerter_der_handliste_nach() {
        for wort in ["Medien", "Salz", "Weiteres", "Messe"] {
            assert!(
                is_ordinary_german_word(wort),
                "{wort} ist gewoehnliches Deutsch"
            );
            assert!(
                !is_common_german_word(wort),
                "{wort} steht nicht in der Handliste -- sonst prueft dieser Test die Datei gar nicht"
            );
        }
    }

    /// Die Bedingung, an der die drei anderen Quellen gescheitert sind.
    ///
    /// Eine Wortliste mit Eigennamen darin macht den Vorschlags-Lauf
    /// SCHLECHTER: steht ein Nachname drin, stirbt der belegte Vorschlag, der
    /// ihn aus seiner Verhoerung zurueckholt -- und zwar stumm.
    ///
    /// Die Namen hier sind bewusst beliebig gewaehlt und bezeichnen niemanden
    /// aus dem Umfeld des Betreibers (ZICK-252). Geprueft wird die
    /// EIGENSCHAFT der Quelle, nicht ein einzelner Mensch: die Baumbank
    /// markiert Eigennamen als eigene Wortart, und die Datei entsteht aus dem
    /// Rest. Wer sie eines Tages aus einer Quelle ohne diesen Filter neu baut,
    /// wird hier rot.
    #[test]
    fn die_grosse_wortliste_traegt_keine_eigennamen() {
        for name in [
            "Andreas",
            "Katrin",
            "Wolfgang",
            "Friedhelm",
            "Ingeborg",
            "Bremerhaven",
        ] {
            assert!(
                !is_ordinary_german_word(name),
                "{name} ist ein Eigenname und muss vorschlagbar bleiben"
            );
        }
    }

    /// Die Trennung der beiden Pruefungen, an ihrem teuersten Fall.
    ///
    /// Beim ersten Uebersetzen mit der grossen Liste galt "Nord Werk" als
    /// gewoehnliche Sprache, weil beide Haelften fuer sich deutsche Woerter
    /// sind. Damit waere jeder zweiwortige Eintrag des Betreibers tot gewesen --
    /// also die Verhoerung eines zusammengesetzten Namens, und das ist der
    /// Regelfall, nicht der Randfall.
    ///
    /// Deshalb steht hinter der Wortfolgen-Pruefung weiterhin NUR die
    /// Handliste. Wer die grosse Liste dort einhaengt, wird hier rot.
    #[test]
    fn die_wortfolgen_pruefung_bleibt_bei_der_handliste() {
        // Beide Haelften sind fuer sich gewoehnliches Deutsch -- gemessen:
        // beide stehen in wortliste-de.txt und in KEINER Handliste-Zeile.
        assert!(is_ordinary_german_word("Nord"));
        assert!(is_ordinary_german_word("Werk"));
        // ... die Folge ist es trotzdem nicht.
        assert!(!is_all_common_german_phrase("Nord Werk"));
    }

    #[test]
    fn leere_eingabe_ist_kein_wort() {
        assert!(!is_common_german_word(""));
        assert!(!is_common_german_word("---"));
        assert!(!is_common_german_word("42"));
    }
}
