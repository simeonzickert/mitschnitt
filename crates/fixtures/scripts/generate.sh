#!/usr/bin/env bash
#
# Erzeugt die Testtoene dieses Crates neu. Alles entsteht auf diesem Rechner,
# nichts stammt aus einer fremden Aufnahme -- deshalb gibt es hier keine
# Lizenzfrage. Braucht macOS (`say`, `afconvert`) und ffmpeg.
#
#   crates/fixtures/scripts/generate.sh
#
# Die Ausgabe ist bitgleich reproduzierbar, solange dieselben Systemstimmen
# vorhanden sind (Anna fuer Deutsch, Samantha fuer Englisch).

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
OUT="audio"
mkdir -p "$OUT"

DE_TEXT="Guten Morgen. Wir hatten gestern eine kurze Besprechung ueber den Zeitplan. Der Vorschlag liegt vor, die Freigabe fehlt noch."
EN_TEXT="Good morning. We had a short meeting yesterday about the schedule. The proposal is ready, the approval is still missing."

say -v Anna     -o "$OUT/speech-de.wav" --data-format=LEI16@16000 "$DE_TEXT"
say -v Samantha -o "$OUT/speech-en.wav" --data-format=LEI16@16000 "$EN_TEXT"

# Die Formatmatrix stammt aus EINER Quelle. Wer hier neue Aufnahmen erfaende,
# haette pro Format ein anderes Signal und koennte nichts mehr vergleichen.
SRC="$OUT/speech-de.wav"
ffmpeg -y -loglevel error -i "$SRC" -c:a libmp3lame -q:a 5 "$OUT/speech-de.mp3"
ffmpeg -y -loglevel error -i "$SRC" -c:a aac -b:a 64k "$OUT/speech-de.m4a"
ffmpeg -y -loglevel error -i "$SRC" -c:a aac -b:a 64k -f mp4 "$OUT/speech-de.mp4"
ffmpeg -y -loglevel error -i "$SRC" -c:a aac -b:a 64k -f adts "$OUT/speech-de.aac"
ffmpeg -y -loglevel error -i "$SRC" -c:a libvorbis -q:a 3 "$OUT/speech-de.ogg"
ffmpeg -y -loglevel error -i "$SRC" -c:a flac "$OUT/speech-de.flac"
ffmpeg -y -loglevel error -i "$SRC" -c:a pcm_s16be "$OUT/speech-de.aiff"
afconvert -f caff -d LEI16@16000 "$SRC" "$OUT/speech-de.caf"

# Echtes Stereo mit UNTERSCHIEDLICHEN Kanaelen: links deutsch, rechts englisch.
# Zwei gleiche Kanaele wuerden auch dann noch stereo aussehen, wenn ein Kodierer
# still auf mono zusammenfaellt.
ffmpeg -y -loglevel error -i "$OUT/speech-de.wav" -i "$OUT/speech-en.wav" \
  -filter_complex "[0:a][1:a]amerge=inputs=2[a]" -map "[a]" -ac 2 \
  -c:a libmp3lame -q:a 5 "$OUT/speech-stereo.mp3"

# Abtastraten-Leiter fuer den Resampler, alle aus derselben Quelle. Bewusst nur
# die ersten 1,5 Sekunden: der Resampler wird an der Rate gemessen, nicht an der
# Laenge, und sechs volle Stuecke waeren allein zwei Drittel des Platzbudgets.
for rate in 8000 16000 22050 32000 44100 48000; do
  ffmpeg -y -loglevel error -i "$SRC" -t 1.5 -ar "$rate" -ac 1 "$OUT/rate-$rate.wav"
done

ls -l "$OUT"
du -sh "$OUT"
