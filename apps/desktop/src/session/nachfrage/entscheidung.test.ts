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
    expect(
      brauchtNachfrage({
        eventId: "evt_1",
        event: null,
        bereitsBeantwortet: false,
      }),
    ).toBe(false);
  });

  it("stellt keine Frage, wenn das Event nur im JSON steht", () => {
    expect(
      brauchtNachfrage({
        eventId: null,
        event: { id: "evt_2" },
        bereitsBeantwortet: false,
      }),
    ).toBe(false);
  });

  it("fragt beim spontanen Gespraech ohne Termin", () => {
    expect(
      brauchtNachfrage({
        eventId: null,
        event: null,
        bereitsBeantwortet: false,
      }),
    ).toBe(true);
  });

  it.each([
    ["leerer Fremdschluessel", { eventId: "", event: null }],
    ["Leerzeichen als Fremdschluessel", { eventId: "   ", event: null }],
    ["Event ohne id", { eventId: null, event: {} }],
    ["Event mit leerer id", { eventId: null, event: { id: "" } }],
    ["Event mit null-id", { eventId: null, event: { id: null } }],
  ])("wertet %s nicht als Termin", (_name, stand) => {
    const vollerStand = { ...stand, bereitsBeantwortet: false };
    expect(hatKalendertermin(vollerStand)).toBe(false);
    expect(brauchtNachfrage(vollerStand)).toBe(true);
  });

  // Zweiter Fall derselben Regel (26.09.2026): einmal pro Gespraech, fuer
  // immer. Ein zweiter Stopp derselben Sitzung fragt nicht erneut, auch
  // ohne Kalendertermin.
  it("stellt keine Frage, wenn sie fuer diese Sitzung schon beantwortet wurde", () => {
    expect(
      brauchtNachfrage({
        eventId: null,
        event: null,
        bereitsBeantwortet: true,
      }),
    ).toBe(false);
  });

  // Beide Bedingungen wirken unabhaengig voneinander -- ein Kalendertermin
  // aendert nichts daran, dass "schon beantwortet" allein schon reicht, und
  // umgekehrt.
  it("bleibt bei einem Kalendertermin unabhaengig vom Beantwortet-Merker stumm", () => {
    expect(
      brauchtNachfrage({
        eventId: "evt_1",
        event: null,
        bereitsBeantwortet: true,
      }),
    ).toBe(false);
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
