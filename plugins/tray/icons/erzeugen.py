#!/usr/bin/env python3
"""Erzeugt die sechs Menueleisten-Icons (macOS Template-Bilder) fuer den Tray.

Motiv: die Tonwellen-Balken aus dem aktuellen App-Icon
(apps/desktop/src-tauri/icons/stable/icon.png, Commit b6b7bed172,
vom Eigentuemer freigegeben). Die im Auftrag genannten "zwei ungleichen
Kreise als Kassettenrollen" sind ueberholt -- das App-Icon wurde seither
zweimal ersetzt (Commit 6658dd54f8 "Neues Icon: eine schlichte Welle statt
der zwei Kreise", dann b6b7bed172 auf den heutigen blauen Markenstil).
Die fuenf Balkenhoehen hier sind 1:1 aus den weissen Balken jenes Icons
abgemessen (Verhaeltnis, nicht Pixelgroesse).

Alle sechs Dateien sind reine macOS-Templates: einzige Farbe ist Schwarz
(R=G=B=0), nur die Alpha-Kanal-Deckung variiert. Erreicht durch 4x-Supersampling
(reines Schwarz auf Transparent kann beim Herunterskalieren keine Mischfarbe
erzeugen, weil es nur die eine Farbe gibt) statt Anti-Aliasing-Tricks.

Aufruf: python3 erzeugen.py
Schreibt tray_default.png, tray_degraded.png, tray_update.png,
tray_recording_0/1/2.png direkt in diesen Ordner.
"""

from __future__ import annotations

from PIL import Image, ImageDraw

SIZE = 160
SS = 4  # Supersampling-Faktor
BIG = SIZE * SS

# --- Balken-Geometrie (Basis 160px-Raum), abgeleitet aus dem App-Icon ---
BAR_WIDTH = 14
GAP = 12
N_BARS = 5
TOTAL_WIDTH = N_BARS * BAR_WIDTH + (N_BARS - 1) * GAP  # 118
LEFT_MARGIN = (SIZE - TOTAL_WIDTH) // 2  # 21
CY = SIZE // 2  # 80
CORNER_RADIUS = 3

# Aus icon.png abgemessene Balkenhoehen (60,137,220,155,75 bei Canvas 512,
# Bezug max=220) auf einen 112px-Hoehenraum bei 160px Canvas skaliert --
# 112 deckt sich mit der Bounding-Box-Hoehe des bisherigen tray_default.png
# (y 23-136 = 113px), behaelt also dieselbe optische Gewichtung.
DEFAULT_HEIGHTS = [31, 70, 112, 79, 38]


def scaled(heights: list[int], factor: float) -> list[int]:
    return [max(2, round(h * factor)) for h in heights]


# Drei Animationsstufen fuer die Aufnahme-Pulsierung (250ms-Takt in ext.rs):
# ruhig -> laut -> sehr ruhig -> Schleife, statt einem statischen Icon.
RECORDING_HEIGHTS = [
    scaled(DEFAULT_HEIGHTS, 0.75),  # recording_0: ruhig
    scaled(DEFAULT_HEIGHTS, 1.15),  # recording_1: Spitze
    scaled(DEFAULT_HEIGHTS, 0.45),  # recording_2: sehr ruhig
]


def new_canvas() -> Image.Image:
    return Image.new("RGBA", (BIG, BIG), (0, 0, 0, 0))


def draw_bars(draw: ImageDraw.ImageDraw, heights: list[int]) -> None:
    for i, h in enumerate(heights):
        x0 = (LEFT_MARGIN + i * (BAR_WIDTH + GAP)) * SS
        x1 = x0 + BAR_WIDTH * SS
        y0 = (CY - h / 2) * SS
        y1 = (CY + h / 2) * SS
        draw.rounded_rectangle(
            [x0, y0, x1, y1], radius=CORNER_RADIUS * SS, fill=(0, 0, 0, 255)
        )


def draw_round_line(
    draw: ImageDraw.ImageDraw,
    p0: tuple[float, float],
    p1: tuple[float, float],
    width: float,
) -> None:
    """Linie mit runden Enden (Pillows draw.line hat eckige Enden)."""
    x0, y0 = p0[0] * SS, p0[1] * SS
    x1, y1 = p1[0] * SS, p1[1] * SS
    w = width * SS
    draw.line([x0, y0, x1, y1], fill=(0, 0, 0, 255), width=round(w))
    r = w / 2
    draw.ellipse([x0 - r, y0 - r, x0 + r, y0 + r], fill=(0, 0, 0, 255))
    draw.ellipse([x1 - r, y1 - r, x1 + r, y1 + r], fill=(0, 0, 0, 255))


def render(build: "callable[[ImageDraw.ImageDraw], None]") -> Image.Image:
    canvas = new_canvas()
    draw = ImageDraw.Draw(canvas)
    build(draw)
    return canvas.resize((SIZE, SIZE), Image.LANCZOS)


def make_default() -> Image.Image:
    return render(lambda d: draw_bars(d, DEFAULT_HEIGHTS))


def make_recording(index: int) -> Image.Image:
    return render(lambda d: draw_bars(d, RECORDING_HEIGHTS[index]))


def make_degraded() -> Image.Image:
    def build(d: ImageDraw.ImageDraw) -> None:
        draw_bars(d, DEFAULT_HEIGHTS)
        # Schraeger Balken ueber die gesamte Wellenform: klares
        # Fehler-/Stumm-Signal, das auch bei 18px noch als "durchgestrichen"
        # erkennbar bleibt (analog zu einem Mute-Icon).
        draw_round_line(d, (14, 146), (146, 14), width=12)

    return render(build)


def make_update() -> Image.Image:
    def build(d: ImageDraw.ImageDraw) -> None:
        draw_bars(d, DEFAULT_HEIGHTS)
        # Benachrichtigungs-Punkt oben rechts, ueberschneidungsfrei zu den
        # Balken (Bar 5 beginnt erst bei y=61, der Punkt endet bei y=43).
        cx, cy, r = 138, 28, 15
        d.ellipse(
            [(cx - r) * SS, (cy - r) * SS, (cx + r) * SS, (cy + r) * SS],
            fill=(0, 0, 0, 255),
        )

    return render(build)


def verify(im: Image.Image, name: str) -> None:
    assert im.mode == "RGBA", f"{name}: mode ist {im.mode}, erwartet RGBA"
    assert im.size == (SIZE, SIZE), f"{name}: size ist {im.size}, erwartet {(SIZE, SIZE)}"
    px = im.load()
    bad = []
    visible = 0
    for y in range(SIZE):
        for x in range(SIZE):
            r, g, b, a = px[x, y]
            if a > 0:
                visible += 1
                if (r, g, b) != (0, 0, 0):
                    bad.append((x, y, r, g, b, a))
    if bad:
        raise AssertionError(f"{name}: {len(bad)} Pixel nicht schwarz, z.B. {bad[:5]}")
    print(f"  {name}: RGBA {im.size}, sichtbare Pixel={visible}, alle R=G=B=0 -- OK")


def main() -> None:
    import os

    out_dir = os.path.dirname(os.path.abspath(__file__))
    files = {
        "tray_default.png": make_default(),
        "tray_degraded.png": make_degraded(),
        "tray_update.png": make_update(),
        "tray_recording_0.png": make_recording(0),
        "tray_recording_1.png": make_recording(1),
        "tray_recording_2.png": make_recording(2),
    }
    print("Pruefe + schreibe Icons:")
    for name, im in files.items():
        verify(im, name)
        im.save(os.path.join(out_dir, name))
    print("Fertig.")


if __name__ == "__main__":
    main()
