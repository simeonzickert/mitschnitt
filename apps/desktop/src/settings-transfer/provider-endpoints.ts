import { PROVIDERS as LLM_PROVIDERS } from "~/settings/ai/llm/shared";
import { PROVIDERS as STT_PROVIDERS } from "~/settings/ai/stt/shared";
import type { AiProviderType } from "~/settings/providers";

/**
 * Mitschnitt-Fork (F14). Which address a provider is supposed to talk to.
 *
 * Read off the provider registries the settings screens already use, never
 * copied: a second list of endpoints would drift, and a drifted list here means
 * a real address flagged as suspicious or a hostile one waved through.
 *
 * This exists because a settings bundle carries `base_url` per provider, and an
 * address is a far more dangerous thing to import than it looks. A bundle
 * without any credential -- the kind meant to be handed around freely, no
 * password, readable JSON -- can still point a provider at an attacker's host.
 * The importing machine keeps its own key, as it should, and from then on sends
 * that key and the contents of confidential meetings to whoever wrote the file.
 * So an address that is not the provider's own is something a person has to see
 * and agree to.
 */

function defaults(): Map<string, string> {
  const map = new Map<string, string>();
  for (const [type, providers] of [
    ["llm", LLM_PROVIDERS],
    ["stt", STT_PROVIDERS],
  ] as const) {
    for (const provider of providers) {
      if (provider.baseUrl) {
        map.set(`${type}:${provider.id}`, provider.baseUrl);
      }
    }
  }
  return map;
}

let cached: Map<string, string> | null = null;

/** The address the app itself would use for this provider, if it knows one. */
export function defaultBaseUrl(
  type: AiProviderType,
  providerId: string,
): string | undefined {
  cached ??= defaults();
  return cached.get(`${type}:${providerId}`);
}

/**
 * Whether an imported address is the provider's own.
 *
 * Compared after normalising the shapes a URL can legitimately be written in
 * (trailing slash, letter case of scheme and host), so a bundle cannot dodge
 * the check by adding a slash. Anything that does not parse as a URL is not a
 * match -- being unsure is a reason to ask, not to wave through.
 *
 * An empty address means "the app decides", which is the safe case and counts
 * as the default. A provider the app has no default for (a local runner, a
 * custom entry) can only be answered with "not the known one", which is the
 * honest answer: the address then genuinely is whatever the file says.
 */
export function isDefaultEndpoint(
  type: AiProviderType,
  providerId: string,
  baseUrl: string,
): boolean {
  if (baseUrl.trim() === "") return true;
  const expected = defaultBaseUrl(type, providerId);
  if (!expected) return false;
  const imported = normalize(baseUrl);
  // Both unparseable would otherwise compare equal as `null === null`.
  if (imported === null) return false;
  return imported === normalize(expected);
}

function normalize(value: string): string | null {
  try {
    const url = new URL(value);
    const path = url.pathname.replace(/\/+$/, "");
    return `${url.protocol}//${url.host}${path}${url.search}`;
  } catch {
    return null;
  }
}
