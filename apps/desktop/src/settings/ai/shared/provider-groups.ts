type Groupable = { id: string; displayName: string };

export type ProviderGroups<T> = { primary: T[]; pinned: T[]; others: T[] };

export function groupProviders<T extends Groupable>(
  providers: readonly T[],
  primaryIds: readonly string[],
  options?: { selectedId?: string | null; configuredIds?: readonly string[] },
): ProviderGroups<T> {
  const byId = new Map(providers.map((provider) => [provider.id, provider]));

  const primary: T[] = [];
  const primaryIdSet = new Set<string>();
  for (const id of primaryIds) {
    const provider = byId.get(id);
    if (!provider) {
      continue;
    }
    primary.push(provider);
    primaryIdSet.add(id);
  }

  const others = providers
    .filter((provider) => !primaryIdSet.has(provider.id))
    .sort((a, b) => a.displayName.localeCompare(b.displayName));

  const configuredIdSet = new Set(options?.configuredIds ?? []);
  const pinned = options
    ? others.filter(
        (provider) =>
          provider.id === options.selectedId ||
          configuredIdSet.has(provider.id),
      )
    : [];

  return { primary, pinned, others };
}

export type LocalTopMoreGroups<T> = { local: T[]; top: T[]; more: T[] };

// Fuer eine lange, konfigurierte Anbieterliste: die eingebauten,
// lokal laufenden Anbieter zuerst als EINE Gruppe "Local", darunter eine
// feste, kurze Reihe gaengiger Cloud-Anbieter ("top"), der Rest unter
// "More". "Local" und "top" sind reine ID-Reihenfolgen und bleiben von
// `options` unberuehrt. Innerhalb von "more" stehen -- wenn `options`
// gesetzt ist -- zuerst der AUSGEWAEHLTE Anbieter (hoechstens einer),
// dann die EINGERICHTETEN (alphabetisch untereinander), dann der Rest
// alphabetisch wie bisher; ohne `options` bleibt "more" komplett
// alphabetisch. Zwei `groupProviders`-Aufrufe hintereinander: der erste
// zieht die lokalen Anbieter aus der Liste, der zweite sortiert die feste
// Cloud-Reihe aus dem Rest. Kein Anbieter geht verloren -- was weder in
// `localIds` noch in `topIds` steht, landet in `more`.
export function splitLocalTopMore<T extends Groupable>(
  providers: readonly T[],
  localIds: readonly string[],
  topIds: readonly string[],
  options?: { selectedId?: string | null; configuredIds?: readonly string[] },
): LocalTopMoreGroups<T> {
  const localSplit = groupProviders(providers, localIds);
  const topSplit = groupProviders(localSplit.others, topIds);
  return {
    local: localSplit.primary,
    top: topSplit.primary,
    more: orderBySelectionThenConfigured(topSplit.others, options),
  };
}

// `more` kommt alphabetisch sortiert herein (aus `groupProviders`). Diese
// Funktion stellt NUR um, sie sortiert innerhalb der drei Eimer nicht neu
// -- deshalb bleibt "eingerichtet" und "Rest" jeweils alphabetisch.
function orderBySelectionThenConfigured<T extends Groupable>(
  more: readonly T[],
  options?: { selectedId?: string | null; configuredIds?: readonly string[] },
): T[] {
  if (!options) {
    return [...more];
  }

  const configuredIdSet = new Set(options.configuredIds ?? []);
  const selected: T[] = [];
  const configured: T[] = [];
  const rest: T[] = [];
  for (const provider of more) {
    if (provider.id === options.selectedId) {
      selected.push(provider);
    } else if (configuredIdSet.has(provider.id)) {
      configured.push(provider);
    } else {
      rest.push(provider);
    }
  }

  return [...selected, ...configured, ...rest];
}
