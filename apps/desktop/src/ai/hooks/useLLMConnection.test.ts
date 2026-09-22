import { describe, expect, it } from "vitest";

import {
  normalizeLLMProviderId,
  resolveLLMConnection,
} from "./useLLMConnection";

import type { Provider } from "~/settings/ai/llm/shared";

describe("normalizeLLMProviderId", () => {
  it("maps the legacy hosted provider id to Anarlog", () => {
    expect(normalizeLLMProviderId("hyprnote")).toBe("anarlog");
  });

  it("preserves current provider ids", () => {
    expect(normalizeLLMProviderId("openai")).toBe("openai");
  });
});

// Kimi/Grok-Review 02.09.2026 (F7). Der Eintrag "anarlog" steht mit
// `baseUrl: ""` in der Anbieterliste, und seine Anforderungen (requires_auth,
// requires_entitlement) blocken in diesem Fork nie. Eine Bestandsinstallation
// mit gespeicherter Kennung bekam deshalb eine "erfolgreiche" Verbindung mit
// leerer Basis-URL -- und der OpenAI-kompatible Client haette gegen den
// Webview-Origin gesprochen.
describe("resolveLLMConnection", () => {
  // Gemessen 02.09.2026: die erste Sperre ist die Anbieterliste selbst --
  // `PROVIDERS` filtert "anarlog" (llm/shared.tsx), also findet die Aufloesung
  // keine Definition und liefert keine Verbindung. Der Test haelt das fest,
  // damit ein Merge, der den Filter verliert, hier auffaellt.
  it("does not know the hosted provider of the original any more", () => {
    const result = resolveLLMConnection({
      providerId: "anarlog",
      modelId: "Auto",
      providerConfig: undefined,
    });

    expect(result.conn).toBeNull();
    expect(result.status).toEqual({
      status: "error",
      reason: "provider_not_found",
      providerId: "anarlog",
    });
  });

  // Die zweite Sperre: eine Definition ohne Basis-URL und ohne
  // `requires_config` -- genau der Zustand, in dem "anarlog" vor dem Filter
  // stand -- bekommt keine Verbindung, sondern den lesbaren Konfigurations-
  // Fehler, den die Oberflaeche schon kennt. Vor dem Waechter: status
  // "success" mit baseUrl "".
  it("refuses to hand out a connection without a base URL", () => {
    const ghost: Provider = {
      id: "ghost",
      displayName: "Ghost",
      badge: null,
      icon: null,
      baseUrl: "",
      requirements: [],
    };

    const result = resolveLLMConnection(
      { providerId: "ghost", modelId: "any", providerConfig: undefined },
      [ghost],
    );

    expect(result.conn).toBeNull();
    expect(result.status).toEqual({
      status: "error",
      reason: "missing_config",
      providerId: "ghost",
      missing: ["base_url"],
    });
  });

  it("still connects a provider whose definition carries its base URL", () => {
    const result = resolveLLMConnection({
      providerId: "openai",
      modelId: "gpt-5.6",
      providerConfig: { type: "llm", base_url: "", api_key: "sk-test" },
    });

    expect(result.conn?.baseUrl).toBe("https://api.openai.com/v1");
    expect(result.status.status).toBe("success");
  });

  it("does not demand a base URL from the on-device Apple model", () => {
    const result = resolveLLMConnection({
      providerId: "apple_foundation",
      modelId: "apple-foundation",
      providerConfig: undefined,
    });

    expect(result.conn?.providerId).toBe("apple_foundation");
  });
});
