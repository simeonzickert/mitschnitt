//! Das Urteil des Bauart-Waechters -- getrennt vom Bauskript, damit es
//! pruefbar ist.
//!
//! Eingebunden zweimal: vom Bauskript (`build.rs`, per `#[path]`) und beim
//! Testen von der Bibliothek. Ein Waechter, dessen Logik nur im Bauskript
//! lebt, laeuft in keinem Test -- und genau das war sein Problem.
//!
//! **Warum es diesen Waechter gibt.** Die Swift-Seite wird immer `release`
//! gebaut, unabhaengig von Rusts Bauart. Umgestellt wird das ueber ein
//! FREMDES Implementierungsdetail: swift-rs liest allein die Umgebungsvariable
//! `DEBUG`, um zwischen `swift build -c debug` und `-c release` zu waehlen
//! (Fork `git+yujonglee/swift-rs` rev `41a1605`, `src-rs/build.rs:277-278`).
//! Aendert swift-rs das, faellt der Bau still auf `-Onone` zurueck -- und die
//! App ist am 101-Minuten-Kanal mehr als zehnmal langsamer, ohne dass
//! irgendwo etwas rot wird.
//!
//! **Warum blosse Existenz nicht reicht (Befund 08.09.2026).** Der Waechter
//! pruefte, ob `libsoniqo-swift.a` im Release-Ordner EXISTIERT. Im
//! inkrementellen Bauordner liegt die Datei eines frueheren Laufs aber noch
//! da: swift-rs koennte wieder debug bauen, der Waechter waere zufrieden, und
//! die langsame App ginge raus. Rot geworden waere er nur auf einem frischen
//! Baum -- also im harmlosen Fall. Dieselbe Klasse, die diese Strecke seit
//! einer Woche verfolgt.

use std::time::SystemTime;

/// Was der Waechter ueber den gerade gelaufenen Swift-Bau sagt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SwiftConfigurationVerdict {
    /// Die erwartete Bauart liegt vor, und die andere wurde in diesem Lauf
    /// nicht angefasst.
    Ok,
    /// Die Bibliothek in der erwarteten Bauart fehlt ganz -- swift-rs hat
    /// woandershin gebaut.
    Missing,
    /// Die andere Bauart wurde in DIESEM Lauf geschrieben. Das ist der
    /// Gefahrenfall: swift-rs hat den Schalter nicht mehr gelesen und gerade
    /// eben das Falsche gebaut.
    OtherFreshlyBuilt,
    /// Die andere Bauart ist neuer als die erwartete, stammt aber nicht aus
    /// diesem Lauf. Kein Abbruch, sondern eine Warnung: das ist der normale
    /// Zustand, nachdem jemand einmal bewusst mit
    /// `MITSCHNITT_SWIFT_PROFILE=debug` gebaut hat. Ein Waechter, der dafuer
    /// abbricht, feuert fuer immer und wird ignoriert.
    OtherIsNewer,
}

/// Das Urteil aus drei Zeitstempeln -- ohne Dateisystem, damit es Tests gibt.
///
/// `expected` und `other` sind die Aenderungszeiten von `libsoniqo-swift.a` in
/// der erwarteten und in der anderen Bauart; `None` heisst, die Datei fehlt.
/// `run_started` ist der Beginn dieses Bauskript-Laufs.
///
/// **Ehrlich zur Restluecke:** faellt swift-rs auf die falsche Bauart zurueck
/// UND war deren Bauordner schon aktuell, wird nichts geschrieben und dieser
/// Waechter schweigt. Der Fall setzt voraus, dass in demselben Bauordner
/// vorher schon einmal die falsche Bauart erzeugt wurde; ein reiner
/// Release-Baum hat den Ordner gar nicht. Die Warnung `OtherIsNewer` deckt
/// genau diesen Verdacht ab, ohne den Bau anzuhalten.
pub(crate) fn swift_configuration_verdict(
    expected: Option<SystemTime>,
    other: Option<SystemTime>,
    run_started: SystemTime,
) -> SwiftConfigurationVerdict {
    if let Some(other) = other
        && other >= run_started
    {
        // Zuerst geprueft, und mit Absicht: wurde in diesem Lauf die falsche
        // Bauart geschrieben, ist es egal, ob daneben eine alte richtige liegt.
        return SwiftConfigurationVerdict::OtherFreshlyBuilt;
    }

    let Some(expected) = expected else {
        return SwiftConfigurationVerdict::Missing;
    };

    match other {
        Some(other) if other > expected => SwiftConfigurationVerdict::OtherIsNewer,
        _ => SwiftConfigurationVerdict::Ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn start() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
    }

    fn before(seconds: u64) -> Option<SystemTime> {
        Some(start() - Duration::from_secs(seconds))
    }

    fn after(seconds: u64) -> Option<SystemTime> {
        Some(start() + Duration::from_secs(seconds))
    }

    /// Der Alltag: nur die richtige Bauart liegt da, frisch gebaut.
    #[test]
    fn nur_die_erwartete_bauart_ist_in_ordnung() {
        assert_eq!(
            swift_configuration_verdict(after(5), None, start()),
            SwiftConfigurationVerdict::Ok
        );
    }

    /// Der Fall, den der alte Waechter als einziger fand.
    #[test]
    fn fehlende_bibliothek_ist_ein_abbruch() {
        assert_eq!(
            swift_configuration_verdict(None, None, start()),
            SwiftConfigurationVerdict::Missing
        );
    }

    /// Der Fall, den der alte Waechter NICHT fand und der ihn ueberhaupt
    /// noetig macht: die richtige Bibliothek liegt aus einem frueheren Lauf
    /// noch da, gebaut wurde in diesem Lauf aber die falsche Bauart.
    #[test]
    fn eine_frisch_gebaute_falsche_bauart_ist_ein_abbruch_trotz_alter_richtiger() {
        assert_eq!(
            swift_configuration_verdict(before(3600), after(9), start()),
            SwiftConfigurationVerdict::OtherFreshlyBuilt
        );
    }

    /// Und auch dann, wenn beide angefasst wurden -- eine frisch gebaute
    /// falsche Bauart bleibt ein Abbruch.
    #[test]
    fn eine_frisch_gebaute_falsche_bauart_schlaegt_eine_frisch_gebaute_richtige() {
        assert_eq!(
            swift_configuration_verdict(after(4), after(9), start()),
            SwiftConfigurationVerdict::OtherFreshlyBuilt
        );
    }

    /// Ein alter, bewusster Debug-Bau darf nicht fuer immer abbrechen --
    /// sonst wird der Waechter ignoriert. Er warnt.
    #[test]
    fn eine_aeltere_aber_neuere_falsche_bauart_warnt_nur() {
        assert_eq!(
            swift_configuration_verdict(before(7200), before(60), start()),
            SwiftConfigurationVerdict::OtherIsNewer
        );
    }

    /// Und wenn die richtige Bauart die neuere von beiden ist, ist alles gut.
    #[test]
    fn eine_aeltere_falsche_bauart_stoert_nicht() {
        assert_eq!(
            swift_configuration_verdict(before(60), before(7200), start()),
            SwiftConfigurationVerdict::Ok
        );
    }

    /// Grenzfall an der Schwelle: genau zum Startzeitpunkt geschrieben zaehlt
    /// als "in diesem Lauf". Die sichere Fehlerrichtung ist der Abbruch.
    #[test]
    fn genau_zum_startzeitpunkt_zaehlt_als_frisch() {
        assert_eq!(
            swift_configuration_verdict(before(10), Some(start()), start()),
            SwiftConfigurationVerdict::OtherFreshlyBuilt
        );
    }
}
