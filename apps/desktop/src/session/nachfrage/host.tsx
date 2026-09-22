import { useCallback, useEffect, useState } from "react";

import { NachfrageDialog } from "./dialog";
import { frageranmelden, nachfrageErledigt, useNachfrageStore } from "./gate";
import { nachfrageSpeichern } from "./speichern";
import { fremdeTeilnehmerNamen } from "./teilnehmer";

import { createHuman } from "~/contacts/queries";
import { useSession, useUpdateSession } from "~/session/queries";
import {
  addSessionParticipant,
  useSessionParticipants,
} from "~/session/queries/participants";

/**
 * Verdrahtet die Frage: meldet sich als Frager an, holt Titel und Teilnehmer
 * der betroffenen Sitzung, schreibt die Antwort weg und gibt die
 * Zusammenfassung frei.
 *
 * Wird im Hauptfenster einmal montiert (`main.tsx`), nicht pro Sitzung: die
 * Frage gehoert zur Aufnahme, nicht zum gerade sichtbaren Reiter.
 */
export function NachfrageHost() {
  const offen = useNachfrageStore((zustand) => zustand.offen);
  const sessionId = offen?.sessionId ?? null;

  useEffect(() => frageranmelden(), []);

  return sessionId ? (
    <NachfrageFuerSitzung key={sessionId} sessionId={sessionId} />
  ) : null;
}

function NachfrageFuerSitzung({ sessionId }: { sessionId: string }) {
  const [laeuft, setLaeuft] = useState(false);
  const [zeitpunkt] = useState(() => new Date());
  const session = useSession(sessionId);
  const teilnehmer = useSessionParticipants(sessionId);
  const updateSession = useUpdateSession(sessionId);

  const selbstHumanId = session?.user_id ?? null;
  const teilnehmerNamen = fremdeTeilnehmerNamen(teilnehmer, selbstHumanId);
  const titelVorher = session?.title ?? "";

  const verwerfen = useCallback(() => {
    nachfrageErledigt(sessionId);
  }, [sessionId]);

  const uebernehmen = useCallback(
    (antwort: { titel: string }) => {
      setLaeuft(true);
      void nachfrageSpeichern(
        {
          sessionId,
          ownerUserId: selbstHumanId,
          titel: antwort.titel,
          titelVorher,
          // Die Teilnehmer schreibt der uebernommene Block aus dem
          // Metadaten-Popover selbst, sofort beim Auswaehlen. Hier bleibt der
          // Titel.
          neueNamen: [],
          vorhandeneNamen: teilnehmerNamen,
        },
        {
          titelSetzen: async (_id, titel) => {
            await updateSession({ title: titel });
          },
          menschAnlegen: ({ ownerUserId, name }) =>
            createHuman({
              ownerUserId,
              name,
              entryPoint: "session_participants",
            }),
          teilnehmerAnhaengen: (id, humanId) =>
            addSessionParticipant(id, humanId),
        },
      ).finally(() => {
        setLaeuft(false);
        // Die Zusammenfassung wartet auf diese Zeile, in jedem Ausgang.
        nachfrageErledigt(sessionId);
      });
    },
    [
      sessionId,
      selbstHumanId,
      titelVorher,
      teilnehmerNamen.join(" "),
      updateSession,
    ],
  );

  return (
    <NachfrageDialog
      offen
      sessionId={sessionId}
      teilnehmerNamen={teilnehmerNamen}
      titelVorher={titelVorher}
      zeitpunkt={zeitpunkt}
      laeuft={laeuft}
      onUebernehmen={uebernehmen}
      onVerwerfen={verwerfen}
    />
  );
}
