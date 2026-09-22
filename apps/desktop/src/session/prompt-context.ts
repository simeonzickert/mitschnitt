/**
 * Der Kontext-Block, den ein Prompt ueber die Sitzung bekommt: Kalendertitel,
 * Zeitraum, Teilnehmer.
 *
 * Steht hier und nicht in `enhance-transform`, weil ihn seit dem 03.09.2026
 * ZWEI Aufgaben brauchen. Die Zusammenfassung hatte ihn seit jeher, der Titel
 * nicht -- und genau deshalb konnte das Titel-Modell aus einer verhoerten
 * Fassung von "Sarec" den erfundenen Namen "Serredi" bauen: es hatte nichts,
 * woran es ihn haette pruefen koennen.
 */

import type { Participant, Session } from "@anlg/plugin-template";
import { sessionEventSchema } from "@anlg/store";

import type { SessionContentSnapshot } from "~/session/content-queries";

export function getSessionData(snapshot: SessionContentSnapshot): Session {
  const parsed = sessionEventSchema.safeParse(snapshot.event);
  if (parsed.success) {
    const eventTitle = parsed.data.title;
    return {
      title: eventTitle || snapshot.title || null,
      startedAt: parsed.data.started_at ?? null,
      endedAt: parsed.data.ended_at ?? null,
      event: {
        name: eventTitle || snapshot.title || "",
      },
    };
  }

  return {
    title: snapshot.title || null,
    startedAt: null,
    endedAt: null,
    event: null,
  };
}

export function getParticipants(
  snapshot: SessionContentSnapshot,
): Participant[] {
  return snapshot.participants
    .filter((participant) => participant.name)
    .map((participant) => ({
      name: participant.name,
      jobTitle: participant.jobTitle || null,
    }));
}
