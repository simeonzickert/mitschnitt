import { describe, expect, it } from "vitest";

import {
  brauchtNachfrage,
  hatKalendertermin,
  titelVorschlag,
} from "./entscheidung";

describe("brauchtNachfrage", () => {
  // Die wichtigste Bedingung der ganzen Strecke: eine Rueckfrage, die auch
  // dann kommt, wenn die Antwort schon dasteht, wird weggeklickt und ist
  // damit wertlos.
  it("stellt keine Frage, wenn ein Kalendertermin verknuepft ist", () => {
    expect(brauchtNachfrage({ eventId: "evt_1", event: null })).toBe(false);
  });

  it("stellt keine Frage, wenn das Event nur im JSON steht", () => {
    expect(brauchtNachfrage({ eventId: null, event: { id: "evt_2" } })).toBe(
      false,
    );
  });

  it("fragt beim spontanen Gespraech ohne Termin", () => {
    expect(brauchtNachfrage({ eventId: null, event: null })).toBe(true);
  });

  it.each([
    ["leerer Fremdschluessel", { eventId: "", event: null }],
    ["Leerzeichen als Fremdschluessel", { eventId: "   ", event: null }],
    ["Event ohne id", { eventId: null, event: {} }],
    ["Event mit leerer id", { eventId: null, event: { id: "" } }],
    ["Event mit null-id", { eventId: null, event: { id: null } }],
  ])("wertet %s nicht als Termin", (_name, stand) => {
    expect(hatKalendertermin(stand)).toBe(false);
    expect(brauchtNachfrage(stand)).toBe(true);
  });
});

describe("titelVorschlag", () => {
  it("behaelt einen vorhandenen Titel, auch wenn Teilnehmer dastehen", () => {
    expect(
      titelVorschlag({ titel: "Jour fixe", teilnehmer: ["Anna Beispiel"] }),
    ).toEqual({ art: "vorhanden", titel: "Jour fixe" });
  });

  it("schlaegt den einen Teilnehmer vor, wenn der Titel leer ist", () => {
    expect(
      titelVorschlag({ titel: "  ", teilnehmer: ["Anna Beispiel"] }),
    ).toEqual({ art: "mit-einem", name: "Anna Beispiel" });
  });

  it("zaehlt bei mehreren Teilnehmern die uebrigen", () => {
    expect(
      titelVorschlag({
        titel: "",
        teilnehmer: ["Anna Beispiel", "Bert Muster", "Cem Yilmaz"],
      }),
    ).toEqual({ art: "mit-mehreren", name: "Anna Beispiel", weitere: 2 });
  });

  it("faellt auf den Zeitpunkt zurueck, wenn nichts vorliegt", () => {
    expect(titelVorschlag({ titel: "", teilnehmer: ["", "  "] })).toEqual({
      art: "nur-zeit",
    });
  });
});
