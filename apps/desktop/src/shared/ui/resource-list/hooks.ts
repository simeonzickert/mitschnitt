import { useQuery } from "@tanstack/react-query";

// Hier stand ein Abruf auf `https://anarlog.so/api/<endpoint>`: die App holte
// die Vorlagen-Vorschlaege ("Web templates" im Vorlagen-Reiter und im
// Vorlagen-Waehler) zur Laufzeit vom Server des Originals. Das ist derselbe
// Fund wie beim Modell-Speicher -- ein Fork, der Inhalte ueber fremde
// Infrastruktur zieht, haengt an einer Leitung, die ihm niemand schuldet, und
// hier fliesst zusaetzlich bei jedem Oeffnen ein Aufruf dorthin ab.
//
// Der Abruf ist entfernt, der Rueckgabewert bleibt bewusst gleich (leere
// Liste): genau das lieferte der alte Code schon bei jeder nicht-ok Antwort,
// die Oberflaeche kommt damit nachweislich zurecht.
//
// OFFEN (der Betreiber entscheidet): entweder die "Web templates"-Fläche ganz
// ausbauen oder auf eine eigene Quelle zeigen. Bis dahin ist der Zweig
// verdrahtet, aber immer leer.
export function useWebResources<T>(_endpoint: string) {
  return useQuery({
    queryKey: ["settings", _endpoint, "suggestions"],
    queryFn: async (): Promise<T[]> => [],
  });
}
