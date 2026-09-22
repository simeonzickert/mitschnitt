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
