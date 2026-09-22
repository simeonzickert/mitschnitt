import { afterEach, describe, expect, it, vi } from "vitest";

import {
  capLoggedMessage,
  captureOperationalError,
  normalizeOperationalError,
  operationalErrorMetadata,
  redactSensitiveText,
} from "./error-reporting";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("normalizeOperationalError", () => {
  it("preserves useful fields from structured API failures", () => {
    expect(
      normalizeOperationalError(
        {
          status: 403,
          code: "subscription_required",
          message: "A Pro subscription is required",
          responseBody: "private response body",
        },
        "integration_connect",
      ).message,
    ).toBe(
      "integration_connect failed (status=403, code=subscription_required, message=A Pro subscription is required)",
    );
  });

  it("keeps primitive failures and ignores arbitrary object data", () => {
    expect(
      normalizeOperationalError("connection refused", "sync").message,
    ).toBe("sync failed: connection refused");
    expect(
      normalizeOperationalError({ transcript: "private transcript" }, "sync")
        .message,
    ).toBe("sync failed");
  });

  it("extracts only privacy-safe operational diagnostics", () => {
    expect(
      operationalErrorMetadata({
        name: "ApiError",
        code: "subscription_required",
        stage: "billing_check",
        statusCode: 403,
        message: "private@example.com",
      }),
    ).toEqual({
      type: "ApiError",
      code: "subscription_required",
      stage: "billing_check",
      status: 403,
    });
    expect(
      operationalErrorMetadata({
        code: "private email@example.com",
        stage: "billing check",
        status: 999,
      }),
    ).toEqual({
      type: "Error",
      code: undefined,
      stage: undefined,
      status: undefined,
    });
  });
});

describe("captureOperationalError", () => {
  // Der einzige Empfaenger ist die Konsole (und ueber die Webview-Bruecke die
  // Logdatei). Vor dem Umbau lief derselbe Aufruf in Sentry.withScope und
  // schrieb nichts in die Konsole -- dieser Test war damals rot.
  it("writes the operation, the normalized error and its metadata to the console", () => {
    const write = vi.spyOn(console, "error").mockImplementation(() => {});

    captureOperationalError(
      { name: "ApiError", code: "subscription_required", status: 403 },
      { operation: "integration_connect", tags: { provider: "google" } },
    );

    expect(write).toHaveBeenCalledOnce();
    const [label, exception, details] = write.mock.calls[0];
    expect(label).toBe("[integration_connect]");
    expect(exception).toBeInstanceOf(Error);
    expect((exception as Error).message).toBe(
      "integration_connect failed (status=403, code=subscription_required)",
    );
    expect(details).toEqual({
      operation: "integration_connect",
      message:
        "integration_connect failed (status=403, code=subscription_required)",
      type: "ApiError",
      code: "subscription_required",
      stage: undefined,
      status: 403,
      provider: "google",
    });
  });

  it("routes warnings to console.warn", () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});

    captureOperationalError(new Error("slow"), {
      operation: "sync",
      level: "warning",
    });

    expect(warn).toHaveBeenCalledOnce();
    expect(error).not.toHaveBeenCalled();
  });
});

// Was die Webview-Bruecke (plugins/tracing, JS_INIT_SCRIPT) an Rust uebergibt,
// ist NICHT das, was console.error bekommt: `invoke('plugin:tracing|do_log',
// { data: args })` laeuft durch JSON.stringify. Ein Error-Objekt hat keine
// aufzaehlbaren Eigenschaften und kommt als `{}` an. Diese Tests pruefen die
// serialisierte Form -- die einzige, die in app.log landet.
describe("what reaches the log file through the bridge", () => {
  function bridgePayload(): () => string {
    const write = vi.spyOn(console, "error").mockImplementation(() => {});
    return () => JSON.stringify(write.mock.calls[0]);
  }

  it("carries the error message as a plain string, not only inside the Error object", () => {
    const payload = bridgePayload();

    captureOperationalError(new Error("boom"), { operation: "sync" });

    // Falsifikator: `new Error("boom")` serialisiert zu `{}` -- ohne eine
    // explizite String-Kopie steht in app.log nur "[sync] {}".
    expect(payload()).toContain("boom");
  });

  it("lets only allowlisted tag and context keys through", () => {
    const payload = bridgePayload();

    captureOperationalError(new Error("boom"), {
      operation: "sync",
      tags: { provider: "google", note: "free text about a customer" },
      context: {
        enabled: true,
        transcript: "private transcript",
        url: "https://example.com/private",
      },
    });

    const serialized = payload();
    expect(serialized).toContain('"provider":"google"');
    expect(serialized).toContain('"enabled":true');
    // Falsifikator: ein `context.transcript` kommt durch.
    expect(serialized).not.toContain("private transcript");
    expect(serialized).not.toContain("free text about a customer");
    expect(serialized).not.toContain("example.com");
    expect(serialized).not.toContain("transcript");
    expect(serialized).not.toContain("url");
  });

  // C2 (Review 02.09.2026): die Allowlist prueft nur Strings -- ein Objekt
  // unter einem erlaubten Schluessel lief durch. Die Typen sagen "Primitiv",
  // aber ein `as`-Cast an der Aufrufstelle sagt gar nichts.
  it("drops an object under an allowlisted key", () => {
    const payload = bridgePayload();

    captureOperationalError(new Error("boom"), {
      operation: "sync",
      context: { provider: { token: "sk-live-abc123" } } as unknown as Record<
        string,
        string
      >,
    });

    const serialized = payload();
    expect(serialized).not.toContain("sk-live-abc123");
    expect(serialized).not.toContain("token");
    expect(serialized).not.toContain("provider");
  });

  it("drops null under an allowlisted key", () => {
    const payload = bridgePayload();

    captureOperationalError(new Error("boom"), {
      operation: "sync",
      context: { enabled: null },
    });

    expect(payload()).not.toContain("enabled");
  });

  it("matches allowlisted keys regardless of case", () => {
    const payload = bridgePayload();

    captureOperationalError(new Error("boom"), {
      operation: "sync",
      tags: { Provider: "google" },
    });

    expect(payload()).toContain('"provider":"google"');
  });

  it("drops allowlisted keys whose value is free text", () => {
    const payload = bridgePayload();

    captureOperationalError(new Error("boom"), {
      operation: "sync",
      tags: { provider: "google mail of alice@example.com" },
    });

    expect(payload()).not.toContain("alice@example.com");
  });
});

// Konsens aller vier Reviews (Opus, Forge, Grok, Kimi, 02.09.2026, C1): die
// Fehlermeldung eines Anbieters traegt oft den Schluessel selbst ("Incorrect
// API key provided: sk-…"), und control.tsx / main.tsx reichen jeden Wurf hier
// durch. Vor dem Umbau kopierte captureOperationalError `exception.message`
// roh in `details` -- der 256-Zeichen-Deckel griff nur fuer Nicht-Error-
// Eingaben. Diese Tests waren rot.
describe("credentials never reach the log file", () => {
  function bridgePayload(): () => string {
    const write = vi.spyOn(console, "error").mockImplementation(() => {});
    return () => JSON.stringify(write.mock.calls[0]);
  }

  it.each([
    [
      "an OpenAI-style key",
      "Incorrect API key provided: sk-live-abcdefghijklmnopqrstuvwxyz",
      "sk-live-abcdefghijklmnopqrstuvwxyz",
      "Incorrect API key provided",
    ],
    [
      "a bearer token",
      "401 for Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.sig",
      "eyJhbGciOiJIUzI1NiJ9",
      "401 for Authorization: Bearer",
    ],
    [
      "a URL query string",
      "GET https://x/y?key=abc failed with 403",
      "key=abc",
      "https://x/y",
    ],
    // D1 (Fix-Runde 1d): the Google live adapter hangs the key on a wss://
    // URL (owhisper-client, google_generative_ai/live.rs).
    [
      "a key on a WebSocket URL",
      "connect wss://generativelanguage.googleapis.com/ws/x?key=AIzaSyD-1234567890abcdefghijklmnop failed",
      "AIzaSyD-1234567890abcdefghijklmnop",
      "wss://generativelanguage.googleapis.com/ws/x?key=[REDACTED] failed",
    ],
    [
      "a Token auth scheme",
      "401 for Authorization: Token abc.def",
      "abc.def",
      "Authorization: Token [REDACTED]",
    ],
    [
      "a quoted JSON secret",
      'body {"api_key":"secret-value-1"} rejected',
      "secret-value-1",
      // The payload is JSON itself, so the quotes arrive escaped.
      '[REDACTED]\\"} rejected',
    ],
    [
      "a colon assignment",
      "config key: hunter2 loaded",
      "hunter2",
      "config key: [REDACTED] loaded",
    ],
    [
      "an api_key parameter",
      "request api_key=secret123 rejected",
      "secret123",
      "rejected",
    ],
    [
      "a token parameter",
      "refresh_token=tok_987 expired",
      "tok_987",
      "expired",
    ],
    [
      "a mail address",
      "user unknown: alice@example.com",
      "alice@example.com",
      "user unknown",
    ],
  ])(
    "strips %s from the message and keeps the rest",
    (_label, message, secret, kept) => {
      const payload = bridgePayload();

      captureOperationalError(new Error(message), {
        operation: "stt_connect",
      });

      expect(payload()).not.toContain(secret);
      expect(payload()).toContain(kept);
    },
  );

  it("caps the message at 512 characters", () => {
    const payload = bridgePayload();

    captureOperationalError(new Error("x".repeat(2000)), {
      operation: "sync",
    });

    const [, , details] = JSON.parse(payload()) as [
      string,
      unknown,
      { message: string },
    ];
    expect(details.message.length).toBeLessThanOrEqual(513);
    expect(details.message.startsWith("x".repeat(512))).toBe(true);
  });

  // Ein Error-Objekt serialisiert zu `{}` -- ausser eine Unterklasse haengt
  // eigene aufzaehlbare Felder an (Antwortkoerper, Anfrage-Optionen). Die
  // kommen sonst ungefiltert durch die Bruecke.
  it("drops enumerable fields an Error subclass carries", () => {
    class ApiError extends Error {
      responseBody = "body with sk-live-abcdefghijklmnopqrstuvwxyz";
    }
    const payload = bridgePayload();

    captureOperationalError(new ApiError("boom"), { operation: "sync" });

    expect(payload()).not.toContain("sk-live-abcdefghijklmnopqrstuvwxyz");
    expect(payload()).not.toContain("responseBody");
    expect(payload()).toContain("boom");
  });

  // "sk-live-…" besteht die Identifier-Pruefung der Allowlist -- die
  // Redaktion muss deshalb auch ueber die erlaubten Werte laufen.
  it("redacts allowlisted tag values too", () => {
    const payload = bridgePayload();

    captureOperationalError(new Error("boom"), {
      operation: "sync",
      tags: { provider: "sk-live-abcdefghijklmnopqrstuvwxyz" },
    });

    expect(payload()).not.toContain("sk-live-abcdefghijklmnopqrstuvwxyz");
  });

  // D1 (Fix-Runde 1d): name, stack and cause of the Error copy go through the
  // patterns as well. The name is a subclass's choice, the stack carries the
  // message of every frame, and a cause is a second Error nobody looked at.
  it("redacts the name, the stack and the cause of the logged Error", () => {
    const write = vi.spyOn(console, "error").mockImplementation(() => {});
    class SkLiveAbcdefghijklmnopqrstuvwxyzError extends Error {}
    const inner = new Error("inner sk-live-zyxwvutsrqponmlkjihgfedcba");
    const outer = Object.assign(
      new SkLiveAbcdefghijklmnopqrstuvwxyzError("outer"),
      { cause: inner },
    );
    outer.name = "sk-live-abcdefghijklmnopqrstuvwxyz";
    outer.stack = `Error: at https://x/y?key=stack-secret-1\n    at frame`;

    captureOperationalError(outer, { operation: "sync" });

    const logged = write.mock.calls[0]![1] as Error;
    expect(logged.name).toBe("[REDACTED]");
    expect(logged.stack).toContain("https://x/y?key=[REDACTED]");
    expect(logged.stack).not.toContain("stack-secret-1");
    expect(Object.keys(logged)).toEqual([]);
    const cause = (logged as Error & { cause?: Error }).cause;
    expect(cause).toBeInstanceOf(Error);
    expect(cause?.message).toBe("inner [REDACTED]");
    expect(Object.getOwnPropertyDescriptor(logged, "cause")?.enumerable).toBe(
      false,
    );
  });

  it("does not cut the stack, only the message", () => {
    const write = vi.spyOn(console, "error").mockImplementation(() => {});
    const error = new Error("x".repeat(2000));
    error.stack = `Error\n${"    at frame\n".repeat(200)}`;

    captureOperationalError(error, { operation: "sync" });

    const logged = write.mock.calls[0]![1] as Error;
    expect(logged.message).toHaveLength(513);
    expect(logged.stack?.length).toBeGreaterThan(2000);
  });
});

describe("redactSensitiveText", () => {
  it("leaves ordinary text alone", () => {
    expect(redactSensitiveText("risk-based desk-top task-1")).toBe(
      "risk-based desk-top task-1",
    );
  });

  // D1 (Fix-Runde 1d): the reviewers' cases, both directions -- what must go
  // and what must stay readable.
  it.each([
    [
      "wss://generativelanguage.googleapis.com/ws?key=AIzaSyD-1234567890abcdefghijklmnop",
      "wss://generativelanguage.googleapis.com/ws?key=[REDACTED]",
    ],
    ["Authorization: Token abc", "Authorization: Token [REDACTED]"],
    ["Authorization: Basic dXNlcjpwYXNz", "Authorization: Basic [REDACTED]"],
    [
      '{"url":"https://x/y?key=1","status":403}',
      '{"url":"https://x/y?key=[REDACTED]","status":403}',
    ],
    ["sk-build failed", "sk-build failed"],
    ["sk-live-abcdefghijklmnopqrstuvwxyz", "[REDACTED]"],
    ["SK-LIVE-ABCDEFGHIJKLMNOPQRSTUVWXYZ", "[REDACTED]"],
    ["http://127.0.0.1:50060/v1?model=x", "http://127.0.0.1:50060/v1?model=x"],
    ["http://localhost:4040/v1?model=x", "http://localhost:4040/v1?model=x"],
    ["ws://[::1]:9000/listen?model=x", "ws://[::1]:9000/listen?model=x"],
    ["password=hunter2 rejected", "password=[REDACTED] rejected"],
    ["sig=abc123&se=2026", "sig=[REDACTED]&se=2026"],
    ["dbtoken=abc stays", "dbtoken=abc stays"],
    ["the token expired", "the token expired"],
    ["Api-Key=Secret123 rejected", "Api-Key=[REDACTED] rejected"],
    ["(key=abc) done", "(key=[REDACTED]) done"],
  ])("%s -> %s", (input, expected) => {
    expect(redactSensitiveText(input)).toBe(expected);
  });

  // G3 (Grok 6/7, Kimi 2/4-7, Forge 6; Fix-Runde 2). The rule is now the
  // same on every host: a query parameter is redacted by its NAME, never by
  // where the request went. Until this round a loopback host kept its whole
  // query (a Google key on a local proxy went to the log), while every other
  // host lost every value (`model=nova` became unreadable, against what the
  // comment above the patterns promised). And an IPv6 host in brackets
  // matched no URL pattern at all.
  it.each([
    // (a) secret parameters go on loopback too
    [
      "ws://127.0.0.1:8080/stt?key=AIzaSyD-1234567890abcdefghijklmnop",
      "ws://127.0.0.1:8080/stt?key=[REDACTED]",
    ],
    [
      "http://localhost:4040/v1?api_key=abc&model=x",
      "http://localhost:4040/v1?api_key=[REDACTED]&model=x",
    ],
    [
      "ws://[::1]:9000/listen?token=abc",
      "ws://[::1]:9000/listen?token=[REDACTED]",
    ],
    // (b) diagnostic parameters stay readable on every host
    ["https://x/y?key=abc&model=nova", "https://x/y?key=[REDACTED]&model=nova"],
    [
      "wss://api.deepgram.com/v1/listen?model=nova-3&language=de&access_token=abc",
      "wss://api.deepgram.com/v1/listen?model=nova-3&language=de&access_token=[REDACTED]",
    ],
    // (c) IPv6 hosts in brackets
    [
      "wss://[2001:db8::1]/ws?key=abc&model=nova",
      "wss://[2001:db8::1]/ws?key=[REDACTED]&model=nova",
    ],
    [
      "http://[2001:db8::1]:8080/v1?model=x",
      "http://[2001:db8::1]:8080/v1?model=x",
    ],
    // (d) 0.0.0.0 is no exception either; userinfo goes, with or without a query
    [
      "http://0.0.0.0:50060/v1?key=abc&model=x",
      "http://0.0.0.0:50060/v1?key=[REDACTED]&model=x",
    ],
    ["https://user:pass@x/y", "https://[REDACTED]@x/y"],
    [
      "wss://user:pass@x/y?key=abc&model=nova",
      "wss://[REDACTED]@x/y?key=[REDACTED]&model=nova",
    ],
    // (d) auth schemes regardless of case
    ["authorization: bearer abc.def", "authorization: bearer [REDACTED]"],
    ["BEARER eyJhbGciOiJIUzI1NiJ9.x.y", "BEARER [REDACTED]"],
    // (e) sk- from 8 characters on, with a stop list for the words
    ["sk-live-abc123", "[REDACTED]"],
    ["sk-project", "sk-project"],
    ["sk-worker failed", "sk-worker failed"],
    ["sk-abc", "sk-abc"],
    // (f) prose: a scheme or a bare `name:` only with a value that looks
    // like one -- a digit, punctuation, mixed case or length; a header line
    // always
    ["a Bearer token expires", "a Bearer token expires"],
    ["the key: press Enter", "the key: press Enter"],
    ["a secret: keep it simple", "a secret: keep it simple"],
    ["Authorization: Token abc", "Authorization: Token [REDACTED]"],
    ["Bearer abc123", "Bearer [REDACTED]"],
    ["Basic dXNlcjpwYXNz", "Basic [REDACTED]"],
    ["config key: hunter2 loaded", "config key: [REDACTED] loaded"],
    ['{"password":"hunter"}', '{"password":"[REDACTED]"}'],
    ["password: hunter", "password: hunter"],
  ])("%s -> %s", (input, expected) => {
    expect(redactSensitiveText(input)).toBe(expected);
  });

  // The same text is redacted twice on its way out (message, then the
  // details record); a second pass must change nothing.
  it.each([
    "wss://x/ws?key=abc&model=nova",
    "wss://user:pass@x/ws?key=abc",
    "Authorization: Bearer abc.def",
    "config key: hunter2 loaded",
    'body {"api_key":"secret-value-1"} rejected',
    "sk-live-abcdefghijklmnopqrstuvwxyz",
  ])("is idempotent on %s", (input) => {
    const once = redactSensitiveText(input);
    expect(redactSensitiveText(once)).toBe(once);
  });

  it("no longer caps what it redacts", () => {
    expect(redactSensitiveText("y".repeat(600))).toHaveLength(600);
  });
});

describe("capLoggedMessage", () => {
  it("marks a truncated message", () => {
    const capped = capLoggedMessage("y".repeat(600));
    expect(capped).toHaveLength(513);
    expect(capped.endsWith("…")).toBe(true);
  });
});
