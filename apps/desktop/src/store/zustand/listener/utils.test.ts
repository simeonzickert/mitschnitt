import { describe, expect, test } from "vitest";

import { fixSpacingForWords, transformWordEntries } from "./utils";

describe("fixSpacingForWords", () => {
  const testCases = [
    {
      transcript: "Hello",
      input: ["Hello"],
      output: [" Hello"],
    },
    {
      transcript: "Yes. Because we",
      input: ["Yes.", "Because", "we"],
      output: [" Yes.", " Because", " we"],
    },
    {
      transcript: "shouldn't",
      input: ["shouldn", "'t"],
      output: [" shouldn", "'t"],
    },
    {
      transcript: "Yes. Because we shouldn't be false.",
      input: ["Yes.", "Because", "we", "shouldn", "'t", "be", "false."],
      output: [" Yes.", " Because", " we", " shouldn", "'t", " be", " false."],
    },
  ];

  test.each(testCases)(
    "transcript: $transcript",
    ({ transcript, input, output }) => {
      expect(output.join("")).toEqual(` ${transcript}`);

      const actual = fixSpacingForWords(input, transcript);
      expect(actual).toEqual(output);
    },
  );
});

describe("transformWordEntries", () => {
  test("die gemessene Sicherheit eines Wortes ueberlebt bis in die Metadaten", () => {
    // Bis zum 03.09.2026 endete die Wortsicherheit an dieser Stelle: der
    // Erkenner rechnet sie, die Bruecke traegt sie, und hier fiel sie unter den
    // Tisch. Was in der Datenbank landet, ist `metadata` -- steht sie da nicht,
    // ist sie fuer jede spaetere Auswertung nicht vorhanden.
    const [words] = transformWordEntries(
      [
        {
          word: "gemessen",
          start: 1,
          end: 1.5,
          confidence: 0.91,
          measured_confidence: 0.91,
        },
        {
          word: "unsicher",
          start: 1.6,
          end: 2,
          confidence: 0.42,
          measured_confidence: 0.42,
        },
      ],
      "gemessen unsicher",
      0,
    );

    expect(words.map((word) => word.metadata?.confidence)).toEqual([
      0.91, 0.42,
    ]);
    // Die Zeitquelle bleibt daneben stehen -- die Sicherheit verdraengt sie nicht.
    expect(words[0].metadata?.timing).toEqual({ source: "provider_word" });
  });

  test("ein Wort ohne gemessene Sicherheit bekommt keine erfundene", () => {
    // Die Gegenprobe. Eine Vorgabe-Eins waere schlimmer als ein fehlendes Feld:
    // sie sieht aus wie eine Messung.
    const [words] = transformWordEntries(
      [{ word: "ohne", start: 0, end: 0.5 }],
      "ohne",
      0,
    );

    expect(words[0].metadata).not.toHaveProperty("confidence");
  });

  test("die aufgedrueckte Draht-Eins wird NICHT als Messung uebernommen", () => {
    // Der eigentliche Falsifikator dieser Runde. `confidence` ist auf dem
    // Draht ein nackter `f64`: wer keine Messung hat, steht dort mit 1,0.
    // Frueher las diese Funktion genau dieses Feld -- damit war nach dem
    // Speichern "sicher gemessen" von "nichts gewusst" nicht mehr zu
    // unterscheiden, und ein Filter auf NIEDRIGE Sicherheit haette die
    // ungemessenen Woerter uebersprungen, weil sie maximal sicher aussehen.
    //
    // Beide Woerter kommen hier mit derselben 1,0 an. Nur eines wurde
    // gemessen. Genau eines darf die Zahl behalten.
    const [words] = transformWordEntries(
      [
        { word: "geraten", start: 0, end: 0.5, confidence: 1 },
        {
          word: "gemessen",
          start: 0.5,
          end: 1,
          confidence: 1,
          measured_confidence: 1,
        },
      ],
      "geraten gemessen",
      0,
    );

    expect(words[0].metadata).not.toHaveProperty("confidence");
    expect(words[1].metadata?.confidence).toBe(1);
  });
});
