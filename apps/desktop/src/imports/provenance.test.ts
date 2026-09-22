import { describe, expect, it } from "vitest";

import {
  parseSessionImportProvenance,
  parseTranscriptTimingQuality,
  worstTimingQuality,
} from "./provenance";

describe("parseSessionImportProvenance", () => {
  it("reads the folder import with its source and its moment", () => {
    expect(
      parseSessionImportProvenance(
        JSON.stringify({
          import: {
            source_kind: "database",
            source_app: "anarlog",
            source_root: "/Users/example/somewhere",
            run_id: "run-1",
            imported_at: "2026-09-10T08:00:00.000Z",
          },
        }),
      ),
    ).toEqual({
      sourceApp: "anarlog",
      sourceRoot: "/Users/example/somewhere",
      importedAt: "2026-09-10T08:00:00.000Z",
    });
  });

  // Ohne diesen Zweig stuende jedes bereits per Datei importierte Gespraech
  // ohne Herkunft da, obwohl sie in der Datenbank liegt.
  it("still reads the older flat shape the file import writes", () => {
    expect(
      parseSessionImportProvenance(
        JSON.stringify({
          importedFrom: "fathom",
          sourcePath: "/Users/example/export.md",
          sourceUrl: "https://example.invalid/x",
          externalId: "abc",
        }),
      ),
    ).toEqual({
      sourceApp: "fathom",
      sourceRoot: "/Users/example/export.md",
      importedAt: null,
    });
  });

  it("says nothing for a session nobody imported", () => {
    expect(parseSessionImportProvenance(null)).toBeNull();
    expect(parseSessionImportProvenance("")).toBeNull();
    expect(parseSessionImportProvenance("{}")).toBeNull();
    expect(
      parseSessionImportProvenance(JSON.stringify({ somethingElse: 1 })),
    ).toBeNull();
  });

  it("survives metadata that is not an object or not even JSON", () => {
    expect(parseSessionImportProvenance("not json at all")).toBeNull();
    expect(parseSessionImportProvenance("[1,2,3]")).toBeNull();
    expect(parseSessionImportProvenance('"a string"')).toBeNull();
    expect(
      parseSessionImportProvenance(JSON.stringify({ import: 7 })),
    ).toBeNull();
    expect(
      parseSessionImportProvenance(JSON.stringify({ import: {} })),
    ).toBeNull();
  });

  it("does not turn a number or an empty string into a source name", () => {
    expect(
      parseSessionImportProvenance(JSON.stringify({ importedFrom: 42 })),
    ).toBeNull();
    expect(
      parseSessionImportProvenance(JSON.stringify({ importedFrom: "   " })),
    ).toBeNull();
    expect(
      parseSessionImportProvenance(
        JSON.stringify({ import: { source_app: "anarlog", source_root: "" } }),
      ),
    ).toEqual({ sourceApp: "anarlog", sourceRoot: null, importedAt: null });
  });
});

describe("parseTranscriptTimingQuality", () => {
  it("reads the three agreed values", () => {
    for (const value of ["synthetic", "provider", "mixed"] as const) {
      expect(
        parseTranscriptTimingQuality(JSON.stringify({ timing_quality: value })),
      ).toBe(value);
    }
  });

  it("throws away a value nobody agreed on", () => {
    expect(
      parseTranscriptTimingQuality(JSON.stringify({ timing_quality: "great" })),
    ).toBeNull();
    expect(
      parseTranscriptTimingQuality(JSON.stringify({ timing_quality: true })),
    ).toBeNull();
    expect(parseTranscriptTimingQuality("{}")).toBeNull();
    expect(parseTranscriptTimingQuality(null)).toBeNull();
    expect(parseTranscriptTimingQuality("broken")).toBeNull();
  });
});

describe("worstTimingQuality", () => {
  it("says nothing when nothing is known", () => {
    expect(worstTimingQuality([])).toBeNull();
    expect(worstTimingQuality([null, null])).toBeNull();
  });

  it("keeps a single verdict as it is", () => {
    expect(worstTimingQuality(["synthetic"])).toBe("synthetic");
    expect(worstTimingQuality(["provider"])).toBe("provider");
    expect(worstTimingQuality(["mixed"])).toBe("mixed");
  });

  // Der teure Fall: ein gemessenes Transkript daneben darf ein geschaetztes
  // nicht weisswaschen.
  it("never lets a measured transcript hide an estimated one", () => {
    expect(worstTimingQuality(["provider", "synthetic"])).toBe("mixed");
    expect(worstTimingQuality(["provider", "mixed"])).toBe("mixed");
    expect(worstTimingQuality(["synthetic", "synthetic"])).toBe("synthetic");
  });

  it("ignores the unknown ones instead of upgrading the verdict", () => {
    expect(worstTimingQuality([null, "synthetic"])).toBe("synthetic");
    expect(worstTimingQuality(["provider", null])).toBe("provider");
  });
});
