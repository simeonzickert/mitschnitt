# libmp3lame in Mitschnitt — und wie du sie austauschst

Stand: 22. September 2026

Mitschnitt speichert Aufnahmen als MP3. Das erledigt **LAME 3.100**
(`libmp3lame`), eine fremde Bibliothek unter der **GNU Lesser General Public
License, Version 2 oder nach deiner Wahl einer späteren Fassung**. Die
Rust-Anbindung `mp3lame-sys` steht unter der **LGPL Version 3**.

Die LGPL erlaubt, eine solche Bibliothek in ein Programm mit eigener Lizenz
einzubauen. Sie knüpft das an eine Bedingung: **du musst die Bibliothek gegen
eine eigene Fassung austauschen können.** Dieses Dokument sagt, wie das hier
geht.

## Wo die Bibliothek liegt

    Mitschnitt.app/Contents/Frameworks/libmp3lame.0.dylib

Sie ist **nicht** ins Hauptprogramm hineinkompiliert. Das Programm lädt sie zur
Laufzeit unter dem Namen `@rpath/libmp3lame.0.dylib`; gesucht wird in
`Contents/Frameworks`.

Nachsehen kannst du das selbst:

    otool -L "Mitschnitt.app/Contents/MacOS/mitschnitt" | grep mp3lame
    nm -U "Mitschnitt.app/Contents/MacOS/mitschnitt" | grep -c " _lame_"

Die erste Zeile zeigt `@rpath/libmp3lame.0.dylib`, die zweite gibt `0` aus --
`-U` zeigt nur DEFINIERTE Symbole (Code, der im Programm liegt). `nm -a` zeigt
dagegen ALLE Symbole, also auch die neun importierten (die Aufrufe an die
Bibliothek) -- damit gaebe dieselbe Zeile mit `-a` statt `-U` auch im
korrekten, dynamisch gelinkten Fall `9` aus, nicht `0`, und die Probe waere
wertlos (gemessen 22.09.2026, gleicher Fund wie in `scripts/lgpl-gate.sh`).

## Der Quelltext

Der vollständige Quelltext der mitgelieferten Fassung liegt im Projekt unter

    vendor/mp3lame-sys/lame-3.100/

Er ist unverändert der Stand, den das Paket `mp3lame-sys 0.1.11` mitbringt.
Dieselbe Fassung gibt es beim Projekt selbst:
**<https://lame.sourceforge.io/>** (LAME 3.100).

Geändert wurde am Quelltext der Bibliothek **nichts**. Geändert wurde nur, wie
sie gebaut wird: statisch → dynamisch, und der mpg123-Dekoder ist mit
übersetzt. Beides steht mit Begründung im Kopf von
`vendor/mp3lame-sys/build.rs`.

## Austauschen

1. LAME 3.100 (oder eine eigene Fassung) besorgen und als dynamische
   Bibliothek bauen:

        ./configure --enable-shared --disable-static --enable-decoder \
                    --disable-frontend --disable-rpath
        make

2. Den Ladenamen setzen, sonst findet das Programm sie nicht:

        install_name_tool -id @rpath/libmp3lame.0.dylib libmp3lame.0.dylib

3. Die Datei im Bundle ersetzen:

        cp libmp3lame.0.dylib "Mitschnitt.app/Contents/Frameworks/"

4. Das Bundle neu signieren. macOS lädt sonst nichts, was nicht zur Signatur
   passt:

        codesign --force --deep --sign - "Mitschnitt.app"

   `-` ist die Ad-hoc-Signatur; sie genügt auf dem eigenen Rechner.

   **Stand 22.09.2026 (Entscheid des Eigentümers, „ok"):** die notarisierte
   Fassung läuft mit Hardened Runtime. Ohne Ausnahme lädt macOS dann nur
   Bibliotheken, die mit derselben Team-ID signiert sind wie die App — genau
   das würde den Austausch aus diesem Dokument wieder unmöglich machen.
   Deshalb trägt `Entitlements.notar.plist` seit diesem Datum das Recht
   `com.apple.security.cs.disable-library-validation`: eine ad hoc (oder
   anders) signierte eigene Fassung der Dylib wird geladen, ohne dass sie
   dieselbe Team-ID wie das Hauptprogramm tragen muss. Siehe
   `ATTRIBUTIONS.md`, Abschnitt 8, für den Preis dieser Ausnahme.

Die Schnittstelle, gegen die Mitschnitt baut, ist `include/lame.h` aus
LAME 3.100. Eine Fassung, die diese Schnittstelle einhält, läuft ohne
Änderung am Programm.

## Lizenztexte

| Datei | Gilt für |
| --- | --- |
| `LICENSE-LGPL-2.0.txt` | LAME selbst (LGPL 2 oder später), verbatim aus `lame-3.100/COPYING` |
| `LICENSE-LGPL-3.0.txt` | die Rust-Anbindung `mp3lame-sys` |
| `LICENSE-GPL-3.0.txt` | gehört zur LGPL 3, die ohne den GPL-3-Text unvollständig ist |

Vollständige Liste aller fremden Bestandteile: `ATTRIBUTIONS.md`, Abschnitt 8.
