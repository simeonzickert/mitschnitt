import { describe, expect, expectTypeOf, it } from "vitest";

import { PROVIDERS } from "./shared";

// Grok-Review 02.09.2026 (Befund 7): PROVIDERS war auf `readonly
// CalendarProvider[]` gehoben worden, damit `platform === "all"` in der
// Seitenleiste typprueft. Damit war das Literal-Union weg -- `id` wurde zu
// `string`, `platform` zu `"macos" | "all" | undefined`. Die Typ-Zusagen
// hier laufen ueber `tsc --noEmit` (src/ ist im tsconfig-include); vor dem
// Rueckbau waren sie rot.
describe("calendar providers", () => {
  it("keeps the literal types of the authored list", () => {
    expectTypeOf(PROVIDERS[0].id).toEqualTypeOf<"apple">();
    expectTypeOf(PROVIDERS[0].platform).toEqualTypeOf<"macos">();
  });

  it("lists Apple Calendar as the only provider", () => {
    expect(PROVIDERS.map((provider) => provider.id)).toEqual(["apple"]);
  });
});
