import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  billing: {
    isPro: true,
    isUpgradingToPro: false,
    upgradeToPro: vi.fn(),
  },
  vocabularyRiskyAliases: vi.fn(),
  vocabularyProposals: vi.fn(),
  execute: vi.fn(),
  /**
   * Was die ANZEIGE sieht -- und sie zieht ABSICHTLICH nie nach.
   *
   * Genau das ist der Zustand, in dem der Fehler lebt: zwischen dem Klick und
   * dem Zeitpunkt, an dem die Einstellung zurueckkommt, liegt ein Rundlauf.
   * Ein Test, in dem der Stand sofort nachzieht, prueft den Fall nicht.
   */
  einstellungen: {} as Record<string, string[]>,
  /**
   * Was WIRKLICH gespeichert ist -- als JSON-Zeichenkette, wie in der Tabelle.
   *
   * Die beiden laufen hier absichtlich auseinander. Wer beim Schreiben die
   * Anzeige liest statt diesen Stand, verliert den vorigen Klick, und genau
   * das soll rot werden.
   */
  gespeichert: {} as Record<string, string>,
  /** Der Fehler, den der naechste Schreibvorgang werfen soll. */
  schreibfehler: null as Error | null,
  /**
   * Der Fehler, den der naechste Schreibvorgang GENAU DIESES Schluessels
   * werfen soll -- danach ist er verbraucht.
   *
   * Ohne das laesst sich der teuerste Fall der Annahme nicht bauen: der Alias
   * scheitert, das Merken als erledigt wuerde trotzdem laufen.
   */
  fehlerJeSchluessel: {} as Record<string, Error>,
  /**
   * Haelt jeden Schreibvorgang an, bis der Test ihn freigibt.
   *
   * Braucht es fuer die Frage, was die Anzeige sagt, WAEHREND etwas laeuft --
   * ein Zustand, den ein Test ohne Bremse gar nicht betreten kann.
   */
  bremse: null as Promise<void> | null,
  /**
   * Die Schlange, die auch das echte `updateSettingValue` benutzt
   * (`enqueueDatabaseWrite`, Schluessel "app-settings"). Ohne sie liefen zwei
   * schnelle Klicks hier nebeneinander und der Test bewiese das Gegenteil von
   * dem, was das Programm tut.
   */
  schwanz: Promise.resolve() as Promise<unknown>,
}));

// Wer eine Verhoerung verwirft, entscheidet Rust
// (`Vocabulary::risky_aliases`). Die Oberflaeche fragt ihn nur -- und dieser
// Mock ist die Stelle, an der man das sieht.
vi.mock("@anlg/plugin-local-stt", () => ({
  commands: {
    vocabularyRiskyAliases: mocks.vocabularyRiskyAliases,
    vocabularyProposals: mocks.vocabularyProposals,
  },
}));

vi.mock("~/db", () => ({
  liveQueryClient: { execute: mocks.execute },
}));

vi.mock("@lingui/react/macro", () => ({
  Trans: ({ children }: { children?: ReactNode }) => <>{children}</>,
  useLingui: () => ({
    t: (
      input: TemplateStringsArray | { message?: string } | string,
      ...values: unknown[]
    ) => {
      if (typeof input === "string") return input;
      if (Array.isArray(input)) {
        return (input as readonly string[]).reduce(
          (message: string, part: string, index: number) =>
            `${message}${part}${index < values.length ? String(values[index]) : ""}`,
          "",
        );
      }
      return (input as { message?: string }).message ?? "";
    },
  }),
}));

// Das echte `updateSettingValue` liest den gespeicherten Stand INNERHALB der
// Schreib-Schlange und schreibt daraus. Dieser Ersatz tut dasselbe, gegen
// `mocks.gespeichert` -- also kann ein Test sehen, ob der Aufrufer wirklich
// gegen den gespeicherten Stand rechnet oder heimlich gegen die Anzeige.
vi.mock("~/settings/queries", () => ({
  updateSettingValue: (
    schluessel: string,
    rechne: (aktuell: string | undefined) => string,
  ) => {
    const naechste = mocks.schwanz
      .catch(() => {})
      .then(async () => {
        if (mocks.bremse) await mocks.bremse;
        const proSchluessel = mocks.fehlerJeSchluessel[schluessel];
        if (proSchluessel) {
          delete mocks.fehlerJeSchluessel[schluessel];
          throw proSchluessel;
        }
        if (mocks.schreibfehler) {
          const fehler = mocks.schreibfehler;
          mocks.schreibfehler = null;
          throw fehler;
        }
        const wert = rechne(mocks.gespeichert[schluessel]);
        mocks.gespeichert[schluessel] = wert;
        return wert;
      });
    mocks.schwanz = naechste.catch(() => {});
    return naechste;
  },
}));

vi.mock("~/shared/config", () => ({
  useConfigValue: (schluessel: string) => mocks.einstellungen[schluessel] ?? [],
  // Dieselbe Zerlegung wie im Programm: der Schreiber bekommt den
  // gespeicherten Rohwert und muss ihn selbst lesen koennen.
  parseConfigStringList: (wert: unknown) => {
    if (Array.isArray(wert)) return wert;
    if (typeof wert !== "string") return [];
    try {
      const geparst = JSON.parse(wert);
      return Array.isArray(geparst) ? geparst : [];
    } catch {
      return [];
    }
  },
}));

import { DictionarySettings, SettingsDictionary } from "./index";
import { setzeSchreibstatusZurueck } from "./write-status";

import { proposalKey } from "~/stt/proposals";

function renderSettings(element: ReactNode) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>{element}</QueryClientProvider>,
  );
}

/**
 * Die Liste allein, ohne den Schreibweg darum herum.
 *
 * Der gespeicherte Stand ist hier derselbe wie die Anzeige -- fuer diese Tests
 * ist das richtig, denn sie pruefen die Liste und nicht das Zusammenspiel. Was
 * passiert, wenn die beiden auseinanderlaufen, prueft der Abschnitt
 * "SettingsDictionary" ganz unten. Aufgezeichnet wird das ERGEBNIS der
 * Rechnung, nicht die Rechenvorschrift.
 */
function zeigeListe(terms: string[], geschrieben = vi.fn()) {
  renderSettings(
    <DictionarySettings
      terms={terms}
      onSave={(rechne) => {
        geschrieben(rechne(terms));
        return Promise.resolve();
      }}
    />,
  );
  return geschrieben;
}

describe("DictionarySettings", () => {
  beforeEach(() => {
    mocks.billing.isPro = true;
    mocks.billing.isUpgradingToPro = false;
    mocks.billing.upgradeToPro.mockClear();
    mocks.vocabularyRiskyAliases.mockReset();
    mocks.vocabularyRiskyAliases.mockResolvedValue({ status: "ok", data: [] });
  });

  afterEach(cleanup);

  it("shows an empty state and disabled add control", () => {
    zeigeListe([]);

    const input = screen.getByRole("textbox", { name: "Term" });
    const addButton = screen.getByRole("button", {
      name: "Add",
    }) as HTMLButtonElement;
    expect(input).toBeTruthy();
    expect(input.closest("[data-slot='input-group']")?.className).toContain(
      "border-border",
    );
    expect(addButton.disabled).toBe(true);
    expect(addButton.className).toContain("bg-black");
    expect(screen.getByText("Add")).toBeTruthy();
    expect(screen.getByText("Your dictionary is empty")).toBeTruthy();
    expect(
      screen.getByText(
        "Tip: Add teammate names, acronyms, company jargon, and product terms.",
      ),
    ).toBeTruthy();
    expect(
      screen.queryByText(
        "Add names, jargon, and product terms to improve transcription.",
      ),
    ).toBeNull();
    expect(screen.queryByText("FastConformer")).toBeNull();
  });

  it("adds entered terms and keeps them normalized", async () => {
    const onSave = vi.fn();
    zeigeListe(["Anarlog"], onSave);

    fireEvent.change(screen.getByRole("textbox", { name: "Term" }), {
      target: { value: " FastConformer, Parakeet TDT " },
    });
    const addButton = screen.getByRole("button", {
      name: "Add",
    }) as HTMLButtonElement;
    await waitFor(() => expect(addButton.disabled).toBe(false));
    fireEvent.click(addButton);

    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith([
        "Anarlog",
        "FastConformer",
        "Parakeet TDT",
      ]),
    );
  });

  it("removes saved terms", () => {
    const onSave = vi.fn();
    zeigeListe(["Anarlog", "Parakeet TDT"], onSave);

    fireEvent.click(screen.getByRole("button", { name: "Remove Anarlog" }));

    expect(onSave).toHaveBeenCalledWith(["Parakeet TDT"]);
  });

  it("does not enable adding duplicate terms", async () => {
    const onSave = vi.fn();
    zeigeListe(["Anarlog"], onSave);

    fireEvent.change(screen.getByRole("textbox", { name: "Term" }), {
      target: { value: "anarlog" },
    });

    const addButton = screen.getByRole("button", {
      name: "Add",
    }) as HTMLButtonElement;
    await waitFor(() => expect(addButton.disabled).toBe(true));
    fireEvent.click(addButton);
    expect(onSave).not.toHaveBeenCalled();
  });

  it("filters saved terms while typing", async () => {
    zeigeListe(["Anarlog", "FastConformer", "Parakeet TDT"]);

    fireEvent.change(screen.getByRole("textbox", { name: "Term" }), {
      target: { value: "fast" },
    });

    await waitFor(() => expect(screen.getByText("FastConformer")).toBeTruthy());
    expect(screen.queryByText("Anarlog")).toBeNull();
    expect(screen.queryByText("Parakeet TDT")).toBeNull();
  });

  it("speichert eine eingetippte Verhoerung im Format, das der Nachlauf liest", async () => {
    const onSave = vi.fn();
    zeigeListe([], onSave);

    fireEvent.change(screen.getByRole("textbox", { name: "Term" }), {
      target: { value: "Sedacz" },
    });
    fireEvent.change(screen.getByRole("textbox", { name: "Misheard as" }), {
      target: { value: "Sarec, Saredi" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith(["Sedacz => Sarec; Saredi"]),
    );
  });

  it("haengt eine Verhoerung an einen vorhandenen Eintrag an", async () => {
    const onSave = vi.fn();
    zeigeListe(["Sedacz => Sarec"], onSave);

    fireEvent.change(screen.getByRole("textbox", { name: "Term" }), {
      target: { value: "Sedacz" },
    });
    fireEvent.change(screen.getByRole("textbox", { name: "Misheard as" }), {
      target: { value: "Saredi" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith(["Sedacz => Sarec; Saredi"]),
    );
  });

  it("laeuft rund: eine gespeicherte Aliaszeile kommt unveraendert zurueck", () => {
    const onSave = vi.fn();
    zeigeListe(["Sedacz => Sarec; Saredi", "Nordwerk"], onSave);

    expect(screen.getByText("Sedacz")).toBeTruthy();
    expect(screen.getByText("Misheard as: Sarec, Saredi")).toBeTruthy();

    // Entfernen laesst die andere Zeile unangetastet -- inklusive Aliassen.
    fireEvent.click(screen.getByRole("button", { name: "Remove Nordwerk" }));
    expect(onSave).toHaveBeenCalledWith(["Sedacz => Sarec; Saredi"]);
  });

  it("sagt sichtbar, wenn eine Verhoerung gewoehnliches Deutsch ist", async () => {
    // Genau die Antwort, die `Vocabulary::risky_aliases` fuer diese beiden
    // Zeilen gibt (belegt vom Rust-Test in plugins/local-stt).
    mocks.vocabularyRiskyAliases.mockResolvedValue({
      status: "ok",
      data: [{ canonical: "Glinck", alias: "Kling" }],
    });

    zeigeListe(["Glinck => Kling; Glink", "Sedacz => Seda das"]);

    // "Kling" ist gewoehnliches Deutsch -- der Nachlauf lehnt es ab.
    await waitFor(() =>
      expect(
        screen.getByText("Ignored, because it is ordinary German: Kling"),
      ).toBeTruthy(),
    );
    // Gefragt wurde mit den normalisierten Zeilen, nicht mit Bruchstuecken.
    expect(mocks.vocabularyRiskyAliases).toHaveBeenCalledWith([
      "Glinck => Kling; Glink",
      "Sedacz => Seda das",
    ]);
    // "Seda das" steht als Verhoerung da, aber OHNE Warnung: nur eines der
    // beiden Woerter ist gewoehnlich, die Folge als ganze ist es nicht.
    expect(screen.getByText("Misheard as: Seda das")).toBeTruthy();
    expect(
      screen.queryByText(/Ignored, because it is ordinary German: Zaja/),
    ).toBeNull();
    // Und "Glink" ist keine gewoehnliche Form -- die Warnung nennt nur "Kling".
    expect(
      screen.queryByText(/Ignored, because it is ordinary German: .*Glink/),
    ).toBeNull();
  });

  it("verliert keine Verhoerung, wenn derselbe Name zweimal im Bestand steht", async () => {
    // Rust legt genau diese zwei Zeilen zu einem Eintrag zusammen, haelt sie
    // also fuer gueltigen Bestand -- der Import kann sie erzeugen. Ein
    // Map.set haette "Sarec" beim Hinzufuegen eines voellig anderen Begriffs
    // stillschweigend geloescht.
    const onSave = vi.fn();
    zeigeListe(["Sedacz => Sarec", "Sedacz => Seda das"], onSave);

    fireEvent.change(screen.getByRole("textbox", { name: "Term" }), {
      target: { value: "Webflow" },
    });
    const addButton = screen.getByRole("button", {
      name: "Add",
    }) as HTMLButtonElement;
    await waitFor(() => expect(addButton.disabled).toBe(false));
    fireEvent.click(addButton);

    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith([
        "Sedacz => Sarec; Seda das",
        "Webflow",
      ]),
    );
  });

  /**
   * Der getippte Text ueberlebt einen gescheiterten Schreibvorgang.
   *
   * Bis zum 03.09.2026 wurden die Felder geleert, bevor ueberhaupt feststand,
   * ob geschrieben wurde. Wer eine lange Aliasliste eingetippt hatte, sah ein
   * Banner "bitte noch einmal versuchen" ueber zwei leeren Feldern.
   *
   * Wird rot, wenn die Felder vor dem Ergebnis geraeumt werden.
   * Laesst durch: ein Schreibvorgang, der haengen bleibt statt zu scheitern --
   * dann bleibt der Text stehen, was die richtige Fehlerrichtung ist.
   */
  it("behaelt den getippten Text, wenn das Speichern fehlschlaegt", async () => {
    renderSettings(
      <DictionarySettings
        terms={[]}
        onSave={() => Promise.reject(new Error("Platte voll"))}
      />,
    );

    const term = screen.getByRole("textbox", {
      name: "Term",
    }) as HTMLInputElement;
    const aliase = screen.getByRole("textbox", {
      name: "Misheard as",
    }) as HTMLInputElement;
    fireEvent.change(term, { target: { value: "Sedacz" } });
    fireEvent.change(aliase, { target: { value: "Sarec" } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await act(async () => undefined);

    expect(term.value).toBe("Sedacz");
    expect(aliase.value).toBe("Sarec");
  });

  it("leert die Felder, sobald das Speichern durch ist", async () => {
    zeigeListe([]);

    const term = screen.getByRole("textbox", {
      name: "Term",
    }) as HTMLInputElement;
    fireEvent.change(term, { target: { value: "Sedacz" } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() => expect(term.value).toBe(""));
  });

  it("findet einen Eintrag auch ueber seine Verhoerung", () => {
    zeigeListe(["Sedacz => Sarec", "Nordwerk"]);

    fireEvent.change(screen.getByRole("textbox", { name: "Term" }), {
      target: { value: "Sarec" },
    });

    expect(screen.getByText("Sedacz")).toBeTruthy();
    expect(screen.queryByText("Nordwerk")).toBeNull();
  });

  /**
   * Der Anlass der ganzen Bearbeiten-Funktion: der Betreiber trug "serredi" als
   * Verhoerung fuer "Sedacz" ein -- aus einer Zusammenfassung abgeschrieben,
   * nicht aus dem Transkript. Das Wort kommt in keiner echten Verhoerung vor,
   * der Eintrag greift nie. Bis hierher blieb nur Loeschen + Neuanlegen.
   */
  it("korrigiert einen toten Alias, ohne den Eintrag neu anzulegen", async () => {
    const onSave = vi.fn();
    zeigeListe(["Nordwerk", "Sedacz => serredi"], onSave);

    fireEvent.click(screen.getByRole("button", { name: "Edit Sedacz" }));
    const aliasFeld = screen.getByRole("textbox", {
      name: "Misheard as Sedacz",
    }) as HTMLInputElement;
    expect(aliasFeld.value).toBe("serredi");

    fireEvent.change(aliasFeld, {
      target: { value: "Sedatsch, Sedatz" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith([
        "Nordwerk",
        "Sedacz => Sedatsch; Sedatz",
      ]),
    );
    // Die Zeile ist wieder eine Anzeige, kein Eingabefeld mehr.
    expect(
      screen.queryByRole("textbox", { name: "Misheard as Sedacz" }),
    ).toBeNull();
  });

  it("bearbeitet Name UND Verhoerungen in einem Zug", async () => {
    const onSave = vi.fn();
    zeigeListe(["Glinck => Kling"], onSave);

    fireEvent.click(screen.getByRole("button", { name: "Edit Glinck" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Edit Glinck" }), {
      target: { value: "A-NOR" },
    });
    fireEvent.change(
      screen.getByRole("textbox", { name: "Misheard as Glinck" }),
      { target: { value: "Kling, A NOR" } },
    );
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith(["A-NOR => Kling; A NOR"]),
    );
  });

  /**
   * Abbrechen heisst Abbrechen: die halb getippte Aenderung darf weder
   * geschrieben werden noch die Anzeige ueberleben.
   */
  it("verwirft die Aenderung beim Abbrechen, ohne zu speichern", () => {
    const onSave = vi.fn();
    zeigeListe(["Sedacz => serredi"], onSave);

    fireEvent.click(screen.getByRole("button", { name: "Edit Sedacz" }));
    fireEvent.change(
      screen.getByRole("textbox", { name: "Misheard as Sedacz" }),
      { target: { value: "irgendwas ganz anderes" } },
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onSave).not.toHaveBeenCalled();
    // Die Zeile zeigt wieder den ALTEN, unveraenderten Stand -- nicht die
    // halb getippte Aenderung.
    expect(screen.getByText("Misheard as: serredi")).toBeTruthy();
    expect(
      screen.queryByRole("textbox", { name: "Misheard as Sedacz" }),
    ).toBeNull();
  });

  it("Escape bricht die Bearbeitung genauso ab wie der Abbrechen-Knopf", () => {
    const onSave = vi.fn();
    zeigeListe(["Sedacz => serredi"], onSave);

    fireEvent.click(screen.getByRole("button", { name: "Edit Sedacz" }));
    const aliasFeld = screen.getByRole("textbox", {
      name: "Misheard as Sedacz",
    });
    fireEvent.change(aliasFeld, { target: { value: "irgendwas" } });
    fireEvent.keyDown(aliasFeld, { key: "Escape" });

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText("Misheard as: serredi")).toBeTruthy();
  });

  // Zwei Eintraege mit demselben Namen wuerde `Vocabulary::parse` zu einem
  // zusammenlegen und dabei eine Verhoerungsliste stillschweigend
  // verschmelzen -- kein "bearbeitet", sondern Datenverlust.
  it("verweigert das Speichern, wenn der neue Name schon einem ANDEREN Eintrag gehoert", async () => {
    const onSave = vi.fn();
    zeigeListe(["Sedacz => Sarec", "Nordwerk => Nordfall"], onSave);

    fireEvent.click(screen.getByRole("button", { name: "Edit Sedacz" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Edit Sedacz" }), {
      target: { value: "Nordwerk" },
    });

    const saveButton = screen.getByRole("button", {
      name: "Save",
    }) as HTMLButtonElement;
    await waitFor(() => expect(saveButton.disabled).toBe(true));
    fireEvent.click(saveButton);
    expect(onSave).not.toHaveBeenCalled();
  });

  it("deaktiviert Speichern bei leerem Namensfeld", async () => {
    zeigeListe(["Sedacz => Sarec"]);

    fireEvent.click(screen.getByRole("button", { name: "Edit Sedacz" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Edit Sedacz" }), {
      target: { value: "   " },
    });

    const saveButton = screen.getByRole("button", {
      name: "Save",
    }) as HTMLButtonElement;
    await waitFor(() => expect(saveButton.disabled).toBe(true));
  });

  /**
   * Derselbe Grundsatz wie beim Hinzufuegen: ein gescheitertes Speichern darf
   * die Eingabe nicht mitnehmen. Ohne das saehe ein Mensch mit langer
   * Aliasliste ein Banner "bitte noch einmal versuchen" ueber einer Zeile,
   * die wieder wie vorher aussieht.
   */
  it("behaelt die Bearbeitung, wenn das Speichern fehlschlaegt", async () => {
    renderSettings(
      <DictionarySettings
        terms={["Sedacz => serredi"]}
        onSave={() => Promise.reject(new Error("Platte voll"))}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Edit Sedacz" }));
    fireEvent.change(
      screen.getByRole("textbox", { name: "Misheard as Sedacz" }),
      { target: { value: "Sedatsch" } },
    );
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await act(async () => undefined);

    // Neu abgefragt, NICHT die alte Referenz: ein Feld, das den
    // Bearbeitungsmodus verlassen haette, waere aus dem DOM verschwunden --
    // die alte Referenz wuerde das nicht zeigen, sie behielte ihren Wert auch
    // losgeloest vom Dokument.
    const aliasFeld = screen.getByRole("textbox", {
      name: "Misheard as Sedacz",
    }) as HTMLInputElement;
    expect(aliasFeld.value).toBe("Sedatsch");
  });
});
/**
 * Die ganze Seite, mit echten Klicks -- und mit einer Anzeige, die
 * ABSICHTLICH hinterherhinkt.
 *
 * Zwei Fassungen standen hier vorher, beide mit einem optimistischen Vorgriff
 * neben dem gespeicherten Stand. Beide verloren Daten: die erste konnte nichts
 * entfernen, die zweite gab sich nur bei EXAKTER Gleichheit wieder frei und
 * konnte deshalb eine fremde Aenderung fuer immer verdecken. Der Vorgriff ist
 * weg; geprueft wird jetzt, dass der Schreibvorgang gegen den GESPEICHERTEN
 * Stand rechnet, und zwar auch dann, wenn die Anzeige noch etwas anderes zeigt.
 */
describe("SettingsDictionary", () => {
  beforeEach(() => {
    mocks.vocabularyRiskyAliases.mockReset();
    mocks.vocabularyRiskyAliases.mockResolvedValue({ status: "ok", data: [] });
    mocks.vocabularyProposals.mockReset();
    mocks.execute.mockReset();
    mocks.execute.mockResolvedValue([
      {
        session_id: "s1",
        words_json: "[]",
        context_names_json: "[]",
        event_participants_json: "[]",
        context_text: "",
        note_body: "",
        generated_title: "",
      },
    ]);
    mocks.einstellungen = {};
    mocks.gespeichert = {};
    mocks.schreibfehler = null;
    mocks.fehlerJeSchluessel = {};
    mocks.bremse = null;
    mocks.schwanz = Promise.resolve();
    // Der Schreibstatus lebt laenger als eine Ansicht -- genau das ist sein
    // Zweck. Ein Testlauf darf den vorigen trotzdem nicht erben.
    setzeSchreibstatusZurueck();
  });

  afterEach(cleanup);

  const vorschlag = (canonical: string, alias: string) => ({
    kind: "context",
    canonical,
    alias,
    occurrences: 3,
    sessionIds: ["s1"],
    evidence: "…",
    source: "Teilnehmer",
  });

  /** Setzt Anzeige UND gespeicherten Stand auf denselben Ausgangswert. */
  const beginneMit = (terms: string[]) => {
    mocks.einstellungen = { personalization_dictionary_terms: terms };
    mocks.gespeichert = {
      personalization_dictionary_terms: JSON.stringify(terms),
    };
  };

  /**
   * Wartet, bis die Schreib-Schlange leer ist -- auch ueber Ketten hinweg.
   *
   * Ein einzelnes `await` auf den Schwanz reicht seit dem 03.09.2026 nicht
   * mehr: das Merken als erledigt wird erst eingereiht, NACHDEM der
   * Alias-Schreibvorgang durch ist, also einen Mikrotask spaeter. Ein Test,
   * der nur einmal wartet, saehe den zweiten Schreibvorgang nie und bewiese
   * genau das Gegenteil von dem, was er behauptet.
   */
  const warte = async () => {
    for (let runde = 0; runde < 6; runde += 1) {
      const vorher = mocks.schwanz;
      await act(async () => {
        await mocks.schwanz;
      });
      if (mocks.schwanz === vorher) return;
    }
  };

  const geschriebeneListe = () => {
    const roh = mocks.gespeichert.personalization_dictionary_terms;
    return roh === undefined ? null : (JSON.parse(roh) as string[]);
  };

  /** Die verworfenen Paare, so wie sie WIRKLICH gespeichert sind. */
  const verworfeneListe = () => {
    const roh = mocks.gespeichert.personalization_dictionary_dismissed;
    return roh === undefined ? null : (JSON.parse(roh) as string[]);
  };

  async function zeigeSeite(vorschlaege: ReturnType<typeof vorschlag>[]) {
    mocks.vocabularyProposals.mockResolvedValue({
      status: "ok",
      data: vorschlaege,
    });
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const { rerender } = render(
      <QueryClientProvider client={client}>
        <SettingsDictionary />
      </QueryClientProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: /Search suggestions/ }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: `Accept ${vorschlaege[0].alias}` }),
      ).toBeTruthy(),
    );
    /**
     * Der Rundlauf, den im Programm die Live-Abfrage macht: der gespeicherte
     * Stand kommt in die Anzeige. Hier von Hand, damit ein Test ihn WEGLASSEN
     * kann -- und genau die Tests, die ihn weglassen, sind die scharfen.
     */
    return () => {
      mocks.einstellungen = {
        personalization_dictionary_terms: geschriebeneListe() ?? [],
      };
      rerender(
        <QueryClientProvider client={client}>
          <SettingsDictionary />
        </QueryClientProvider>,
      );
    };
  }

  /**
   * Zwei schnelle Klicks, ohne dass die Anzeige dazwischen nachzieht. Der
   * zweite muss den ersten sehen.
   */
  it("verliert den ersten Klick nicht, wenn der zweite sofort folgt", async () => {
    await zeigeSeite([
      vorschlag("Merkentin", "Serkantin"),
      vorschlag("Tofmann", "Tochmann"),
    ]);

    fireEvent.click(screen.getByRole("button", { name: "Accept Serkantin" }));
    fireEvent.click(screen.getByRole("button", { name: "Accept Tochmann" }));
    await warte();

    expect(geschriebeneListe()).toEqual([
      "Merkentin => Serkantin",
      "Tofmann => Tochmann",
    ]);
  });

  /**
   * Loeschen ist dauerhaft: wer einen Vorschlag annimmt und den entstandenen
   * Eintrag danach loescht, bekommt ihn beim naechsten Annehmen nicht zurueck.
   */
  it("holt einen geloeschten Eintrag beim naechsten Annehmen nicht zurueck", async () => {
    const zieheNach = await zeigeSeite([
      vorschlag("Merkentin", "Serkantin"),
      vorschlag("Tofmann", "Tochmann"),
    ]);

    fireEvent.click(screen.getByRole("button", { name: "Accept Serkantin" }));
    await warte();
    zieheNach();
    fireEvent.click(screen.getByRole("button", { name: "Remove Merkentin" }));
    await warte();
    expect(geschriebeneListe()).toEqual([]);

    // Nach dem ersten Annehmen laeuft die Suche neu (der Schluessel der
    // Abfrage traegt Liste und Verworfenes), also erst warten, bis die
    // Vorschlaege wieder dastehen.
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Accept Tochmann" }),
      ).toBeTruthy(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Accept Tochmann" }));
    await warte();
    expect(geschriebeneListe()).toEqual(["Tofmann => Tochmann"]);
  });

  /** Dasselbe eine Ebene tiefer: zwei schnelle Loeschungen. */
  it("macht die erste Loeschung nicht durch die zweite rueckgaengig", async () => {
    beginneMit(["Merkentin", "Tofmann", "Talwiese"]);
    renderSettings(<SettingsDictionary />);

    fireEvent.click(screen.getByRole("button", { name: "Remove Merkentin" }));
    fireEvent.click(screen.getByRole("button", { name: "Remove Tofmann" }));
    await warte();

    expect(geschriebeneListe()).toEqual(["Talwiese"]);
  });

  /**
   * Der Fall, an dem der optimistische Vorgriff starb.
   *
   * Er gab sich nur frei, wenn der gespeicherte Stand seinem eigenen EXAKT
   * entsprach. Eine fremde Aenderung, die dazwischenfunkt, ueberspringt diesen
   * Zwischenstand -- die Live-Abfrage buendelt Tabellenaenderungen und muss ihn
   * nie zeigen. Danach rechnete jeder weitere Klick gegen den eigenen alten
   * Stand und schrieb die fremde Aenderung dauerhaft weg.
   *
   * Hier zieht die Anzeige NIE nach, und der zweite Klick muss trotzdem den
   * fremden Eintrag stehen lassen.
   */
  it("schreibt eine Aenderung von aussen nicht weg", async () => {
    beginneMit(["Merkentin", "Tofmann"]);
    renderSettings(<SettingsDictionary />);

    fireEvent.click(screen.getByRole("button", { name: "Remove Merkentin" }));
    await warte();
    expect(geschriebeneListe()).toEqual(["Tofmann"]);

    // Jemand anders schreibt -- eine andere Ansicht, ein Hintergrundlauf.
    mocks.gespeichert.personalization_dictionary_terms = JSON.stringify([
      "Tofmann",
      "Von aussen",
    ]);

    fireEvent.click(screen.getByRole("button", { name: "Remove Tofmann" }));
    await warte();
    expect(geschriebeneListe()).toEqual(["Von aussen"]);
  });

  /**
   * Ein fehlgeschlagenes Speichern sah bis zum 03.09.2026 aus wie Erfolg.
   *
   * `useSetSettingValue` verschluckte den Fehler in ein `console.error`, die
   * Oberflaeche zeigte den Eintrag als weg -- und beim naechsten Oeffnen war er
   * wieder da. Ein Werkzeug, das schweigt, wenn es kaputt ist, ist schlimmer
   * als eines, das gar nicht laeuft.
   */
  it("sagt es, wenn das Speichern fehlschlaegt", async () => {
    beginneMit(["Merkentin"]);
    mocks.schreibfehler = new Error("Platte voll");
    renderSettings(<SettingsDictionary />);

    expect(screen.queryByText(/The change could not be saved/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Remove Merkentin" }));
    await warte();

    await waitFor(() =>
      expect(screen.getByText(/The change could not be saved/)).toBeTruthy(),
    );
    // Und der gespeicherte Stand ist unangetastet geblieben.
    expect(geschriebeneListe()).toEqual(["Merkentin"]);
  });

  /**
   * DER TEUERSTE FALL DER GANZEN SEITE.
   *
   * Annehmen sind ZWEI Schreibvorgaenge: der Alias ins Woerterbuch, das Paar in
   * die Verworfen-Liste. Bis zum 03.09.2026 liefen sie unabhaengig los, und die
   * Schlange faehrt nach einem Fehlschlag ausdruecklich weiter. Scheiterte der
   * erste, lief der zweite trotzdem: kein Eintrag im Woerterbuch, der Vorschlag
   * aber dauerhaft verworfen und aus dem Postfach verschwunden -- waehrend das
   * Banner daneben sagte, es sei nichts passiert.
   *
   * Wird rot, wenn "verworfen" ohne "Alias steht" passieren kann.
   * Laesst durch: die umgekehrte Reihenfolge (Alias steht, Merken scheitert) --
   * die ist Absicht, siehe den Test darunter.
   */
  it("verwirft einen Vorschlag nicht, wenn sein Alias nicht gespeichert werden konnte", async () => {
    mocks.fehlerJeSchluessel.personalization_dictionary_terms = new Error(
      "Platte voll",
    );
    await zeigeSeite([vorschlag("Merkentin", "Serkantin")]);

    fireEvent.click(screen.getByRole("button", { name: "Accept Serkantin" }));
    await warte();

    expect(geschriebeneListe()).toBeNull();
    expect(verworfeneListe()).toBeNull();
    await waitFor(() =>
      expect(screen.getByText(/The change could not be saved/)).toBeTruthy(),
    );
  });

  /**
   * Die andere Richtung ist ausdruecklich erlaubt und deshalb festgehalten:
   * der Alias steht, das Merken scheitert. Dann kommt der Vorschlag beim
   * naechsten Lauf wieder -- und ihn ein zweites Mal anzunehmen aendert
   * nichts, weil der Alias schon da ist. Die billige Fehlerrichtung.
   */
  it("behaelt den gespeicherten Alias, wenn nur das Merken scheitert", async () => {
    mocks.fehlerJeSchluessel.personalization_dictionary_dismissed = new Error(
      "Platte voll",
    );
    await zeigeSeite([vorschlag("Merkentin", "Serkantin")]);

    fireEvent.click(screen.getByRole("button", { name: "Accept Serkantin" }));
    await warte();

    expect(geschriebeneListe()).toEqual(["Merkentin => Serkantin"]);
    expect(verworfeneListe()).toBeNull();
  });

  /**
   * Ein angenommener Vorschlag wird auch gemerkt -- sonst legt der naechste
   * Lauf ihn wieder vor. Geprueft wird die VERWORFEN-Liste, nicht die
   * Woerterbuchliste: die schnelle Annahme oben sieht nur die zweite, und
   * damit haette das Merken ganz fehlen koennen, ohne dass ein Test rot wird.
   */
  it("merkt einen angenommenen Vorschlag als erledigt", async () => {
    await zeigeSeite([vorschlag("Merkentin", "Serkantin")]);

    fireEvent.click(screen.getByRole("button", { name: "Accept Serkantin" }));
    await warte();

    expect(geschriebeneListe()).toEqual(["Merkentin => Serkantin"]);
    expect(verworfeneListe()).toEqual([proposalKey("Merkentin", "Serkantin")]);
  });

  /**
   * Verwerfen war bis zum 03.09.2026 ueberhaupt nicht geklickt worden -- kein
   * einziger Test fasste den Knopf an, obwohl "Verwerfen ist dauerhaft" eine
   * der beiden Zusagen dieser Seite ist.
   */
  it("merkt einen verworfenen Vorschlag dauerhaft und fasst das Woerterbuch nicht an", async () => {
    beginneMit(["Talwiese"]);
    await zeigeSeite([vorschlag("Merkentin", "Serkantin")]);

    fireEvent.click(screen.getByRole("button", { name: "Dismiss Serkantin" }));
    await warte();

    expect(verworfeneListe()).toEqual([proposalKey("Merkentin", "Serkantin")]);
    expect(geschriebeneListe()).toEqual(["Talwiese"]);
  });

  /**
   * Das Banner sagt etwas ueber ERGEBNISSE, und ein Start ist kein Ergebnis.
   *
   * Bis zum 03.09.2026 wurde es beim BEGINN des naechsten Schreibvorgangs
   * geloescht. Wer nach einem Fehlschlag noch etwas anfasste, sah die Meldung
   * ueber seinen Verlust verschwinden, waehrend ueberhaupt nichts entschieden
   * war -- bliebe der neue Vorgang haengen, waere sie fuer immer weg.
   *
   * Wird rot, wenn das Banner am START eines Schreibvorgangs geraeumt wird.
   */
  it("raeumt das Banner nicht weg, nur weil der naechste Schreibvorgang beginnt", async () => {
    beginneMit(["Merkentin", "Tofmann"]);
    mocks.schreibfehler = new Error("Platte voll");
    renderSettings(<SettingsDictionary />);

    fireEvent.click(screen.getByRole("button", { name: "Remove Merkentin" }));
    await warte();
    await waitFor(() =>
      expect(screen.getByText(/The change could not be saved/)).toBeTruthy(),
    );

    let loese = () => undefined as void;
    mocks.bremse = new Promise<void>((aufloesen) => {
      loese = aufloesen;
    });
    fireEvent.click(screen.getByRole("button", { name: "Remove Tofmann" }));
    await act(async () => undefined);

    expect(screen.getByText(/The change could not be saved/)).toBeTruthy();

    loese();
    mocks.bremse = null;
    await warte();
    // Und der geglueckte Versuch ist die Antwort auf "bitte noch einmal
    // versuchen": jetzt darf es weg.
    await waitFor(() =>
      expect(screen.queryByText(/The change could not be saved/)).toBeNull(),
    );
    expect(geschriebeneListe()).toEqual(["Merkentin"]);
  });

  /**
   * Ein Seitenwechsel nahm das Banner mit: der Zustand lag in der Ansicht, und
   * die Ansicht verschwand. Wer nach einem Fehlschlag woanders hinklickte und
   * zurueckkam, sah eine heile Oberflaeche ueber einem verlorenen Eintrag.
   */
  it("haelt das Banner ueber einen Seitenwechsel hinweg", async () => {
    beginneMit(["Merkentin"]);
    mocks.schreibfehler = new Error("Platte voll");
    const erste = renderSettings(<SettingsDictionary />);

    fireEvent.click(screen.getByRole("button", { name: "Remove Merkentin" }));
    await warte();
    await waitFor(() =>
      expect(screen.getByText(/The change could not be saved/)).toBeTruthy(),
    );

    erste.unmount();
    renderSettings(<SettingsDictionary />);

    expect(screen.getByText(/The change could not be saved/)).toBeTruthy();
  });
});
