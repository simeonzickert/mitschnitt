import { useEffect, useState } from "react";
// @ts-ignore virtual module provided by ./vite.ts
import { latestContent, latestVersion } from "virtual:changelog";

import { processContent } from "@anlg/changelog";

export function getLatestVersion(): string | null {
  return latestVersion;
}

// Bis zum 01.09.2026 stand hier ein Netzabruf auf
// raw.githubusercontent.com/fastrepl/anarlog/.../content/<version>.md: fuer
// jede Version ausser der eingebauten holte die App die Aenderungsliste des
// Originals und zeigte sie dem Nutzer als die eigene. Er ist entfernt. Eigene
// Ausgaben bringen ihre Liste eingebaut mit (packages/changelog/content, siehe
// LIESMICH.md dort); fuer jede andere Version steht in der Oberflaeche
// "No changelog available for this version." -- Absicht, kein Defekt.
export function useChangelogContent(version: string) {
  const [content, setContent] = useState<string | null>(null);
  const [date, setDate] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (version === latestVersion && latestContent) {
      const { content: parsed, date: parsedDate } =
        processContent(latestContent);
      setContent(parsed);
      setDate(parsedDate);
    } else {
      setContent(null);
      setDate(null);
    }
    setLoading(false);
  }, [version]);

  return { content, date, loading };
}
