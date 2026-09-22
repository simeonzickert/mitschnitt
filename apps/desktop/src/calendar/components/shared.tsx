import type { ReactNode } from "react";

export type CalendarProvider = {
  disabled: boolean;
  id: string;
  displayName: string;
  icon: ReactNode;
  badge?: string | null;
  platform?: "macos" | "all";
};

const _PROVIDERS = [
  {
    disabled: false,
    id: "apple",
    displayName: "Apple Calendar",
    badge: "",
    icon: (
      <img
        src="/assets/apple-calendar.png"
        alt="Apple Calendar"
        className="size-5 rounded-[4px] object-cover"
      />
    ),
    platform: "macos",
  },
] as const satisfies readonly CalendarProvider[];

// Bleibt eng: `id` ist "apple", `platform` ist "macos", nicht string. Der
// Vergleich mit "all" lebt in `isProviderAvailable` unten, das gegen den
// deklarierten Typ `CalendarProvider` prueft -- kein Cast, keine Aufweichung
// der Liste (Grok-Review 02.09.2026, Befund 7).
export const PROVIDERS = [..._PROVIDERS];

export function isProviderAvailable(
  provider: CalendarProvider,
  isMacos: boolean,
): boolean {
  return (
    provider.platform === "all" || (provider.platform === "macos" && isMacos)
  );
}
