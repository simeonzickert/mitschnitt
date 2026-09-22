// Die Wortmarke des Forks: dieselbe Welle wie das App-Icon, nur ohne den
// blauen Grund. Die Balkengeometrie ist wortgleich aus
// `apps/desktop/src-tauri/icons/src/mitschnitt-welle.svg` uebernommen, damit
// Icon und Marke nicht auseinanderlaufen; die viewBox schneidet den Rand des
// 1024er-Quadrats weg und laesst rundum 32 Einheiten Luft (Balken liegen
// dort auf x 224..816, y 232..792) -- ohne die Luft stossen die aeusseren
// Balken an den Rand und sehen abgeschnitten aus.
// `fill="currentColor"` ist Absicht: der Ladebildschirm faerbt die Marke ueber
// Textfarben ein und legt eine zweite Kopie als Schimmer darueber.
export const MITSCHNITT_MARK_VIEW_BOX = "192 200 656 624";

export function MitschnittMark({ className }: { className?: string }) {
  return (
    <svg
      viewBox={MITSCHNITT_MARK_VIEW_BOX}
      fill="currentColor"
      className={className}
      aria-hidden="true"
    >
      <rect x="224" y="452" width="80" height="120" />
      <rect x="352" y="356" width="80" height="312" />
      <rect x="480" y="232" width="80" height="560" />
      <rect x="608" y="332" width="80" height="360" />
      <rect x="736" y="428" width="80" height="168" />
    </svg>
  );
}
