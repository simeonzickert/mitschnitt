// Fork: es gibt keinen Empfaenger fuer Fehlerberichte mehr. Bis zum 01.09.2026
// lief dieses Modul auf @sentry/react (nie initialisiert, also stumm) und
// schaltete ueber `set_crash_reporting_enabled` den Sentry-Sender im Rust an
// und aus. Beides ist ausgebaut (ZICK-257, ZICK-260). Geblieben sind die zwei
// Helfer, die aus einem beliebigen Fehlerwert einen lesbaren Fehler machen,
// und `captureOperationalError`, das in die Konsole schreibt -- und damit ueber
// die Webview-Bruecke (plugins/tracing) in die Logdatei, die der Nutzer selbst
// weitergibt.

type ErrorContextValue = null | boolean | number | string;
export type SeverityLevel =
  | "fatal"
  | "error"
  | "warning"
  | "log"
  | "info"
  | "debug";
const SAFE_IDENTIFIER_RE = /^[a-zA-Z0-9_.:/-]{1,128}$/;

function safeIdentifier(value: unknown): string | undefined {
  return typeof value === "string" && SAFE_IDENTIFIER_RE.test(value)
    ? value
    : undefined;
}

/**
 * What a logged string may not carry (Opus/Forge/Grok/Kimi-Review 02.09.2026,
 * C1; sharpened in Fix-Runde 1d, D1, and again in Fix-Runde 2, G3). Provider
 * answers quote the credential they rejected ("Incorrect API key provided:
 * sk-…"), request failures quote the URL including its query, and both reach
 * `captureOperationalError` unfiltered through the error frame in control.tsx
 * and the startup tasks in main.tsx. The Rust side
 * (plugins/tracing/src/redaction.rs) applies the same patterns to every log
 * line, so the two nets match; this one runs first. The WebSocket client
 * (crates/ws-client/src/retry.rs) shares the URL rules for the request it
 * logs on connect.
 *
 * What the patterns do, and why they stop where they stop:
 *
 * - URLs (`http`, `https`, `ws`, `wss` -- the Google live adapter hangs the
 *   key as `?key=` on a `wss://` URL): a query parameter is redacted by its
 *   NAME, on every host. Secret-named ones (`key`, `api_key`, `token`,
 *   `access_token`, `secret`, `sig`, `password`, `jwt`, … -- see
 *   `isSecretParamName`) become [REDACTED]; diagnostic ones (`model`,
 *   `language`, `version`) stay readable, so `?key=…&model=nova` still says
 *   which model was asked for. Until Fix-Runde 2 a loopback host kept its
 *   whole query and every other host lost every value -- the same key on
 *   `127.0.0.1` and on `googleapis.com` is the same secret, and a local
 *   server's `model=` is as useful as a remote one's. Hosts in brackets
 *   (`[::1]`, `[2001:db8::1]`) are an authority like any other; userinfo
 *   (`https://user:pass@host`) goes whole, with or without a query.
 * - Auth schemes: an `Authorization:` header line loses its value whatever
 *   it looks like; a bare `Bearer`/`Token`/`Basic` in running text only when
 *   the word after it looks like a token (a digit, `._=/+`, mixed case, or
 *   20+ characters) -- "a Bearer token expires" is prose. Case does not
 *   matter.
 * - `sk-` keys from 8 characters on, with a stop list for the words that
 *   share the prefix (`sk-build`, `sk-project`, `sk-worker`); a length floor
 *   of 20 let `sk-live-abc123` through.
 * - Named secrets in any assignment form -- `key=…`, `"api_key":"…"`,
 *   `key: …` -- for an explicit list of names. `=` and quoted forms always;
 *   a bare `name: value` only when the value looks like a token, so "the
 *   key: press Enter" stays. No `\w*token` wildcard: it also caught
 *   `dbtoken=` in ordinary text and nothing else worth having.
 *
 * Every pattern's tail stops at whitespace and at `"`, `'`, `)`, `,`, `]`,
 * `}`, so a JSON fragment such as `{"url":"https://x/y?key=1","status":403}`
 * keeps its structure and its status code after the redaction.
 *
 * Order matters: URL queries before userinfo (the authority has to see the
 * `@`), both before the named secrets so a query is handled as a query; the
 * header line before the bare scheme, the schemes before `sk-` so
 * "Bearer sk-…" leaves one marker, not two.
 */
const MAX_LOGGED_MESSAGE_LENGTH = 512;
const TAIL = `[^\\s"'),\\]}]`;
// The authority: a bracketed IPv6 host, or anything up to the `?` that
// starts the query. The query tail keeps `[` and `]` so a second pass reads
// `key=[REDACTED]` as one value and leaves it; a URL inside a JSON array
// still stops at `"`.
const URL_RE = new RegExp(
  `\\b((?:https?|wss?):\\/\\/)((?:\\[[^\\]\\s]*\\]|[^\\s?"'),\\]}\\[])*)\\?([^\\s"'),}]*)`,
  "gi",
);
const URL_USERINFO_RE = new RegExp(
  `\\b((?:https?|wss?):\\/\\/)[^\\s"'),\\]}\\[@/]+@`,
  "gi",
);
// A value may not start with `[`: that is a marker from an earlier pass, and
// the same text is redacted twice on its way out (message, then details).
// Without this a second pass turns `key=[REDACTED]` into `key=[REDACTED]]`.
const VALUE = `[^\\s"'),\\]}\\[]${TAIL}*`;
const AUTH_HEADER_RE = new RegExp(
  `\\b((?:proxy-)?authorization\\s*:\\s*(?:bearer|token|basic))\\s+${VALUE}`,
  "gi",
);
const AUTH_SCHEME_RE = new RegExp(
  `\\b(bearer|token|basic)\\s+(${VALUE})`,
  "gi",
);
const SK_KEY_RE = /\bsk-(?!(?:build|project|worker)\b)[A-Za-z0-9_-]{8,}/gi;
// Bare parameters stop at `&` too, so `sig=…&se=2026` keeps its neighbour.
const NAMED_SECRET_RE = new RegExp(
  "\\b(api[_-]?key|access_token|refresh_token|id_token|password|secret|token|sig|key)" +
    `(\\s*["']?\\s*[:=]\\s*["']?)([^\\s"'),\\]}&\\[][^\\s"'),\\]}&]*)`,
  "gi",
);
const EMAIL_RE = /[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/g;
// A parameter name that carries a secret, by its ending: `key`, `api_key`,
// `x-api-key`, `token`, `id_token`, `client_secret`, `sig`, `jwt` … An
// over-match (`monkey`) costs a readable value, an under-match costs a key.
const SECRET_PARAM_NAME_RE =
  /(key|token|secret|sig|signature|password|passwd|pwd|auth|credentials?|jwt)$/i;

function isSecretParamName(name: string): boolean {
  return SECRET_PARAM_NAME_RE.test(name);
}

/**
 * Whether a bare value is a token rather than a word: a digit, `._=/+`,
 * a lower-case letter followed by an upper-case one (base64), or 20+
 * characters. Sentence punctuation at the end does not count; a hyphen
 * alone does not either ("token-based"), `sk-…` has its own pattern.
 */
function looksLikeSecret(value: string): boolean {
  const core = value.replace(/[.:;!?]+$/, "");
  return (
    /[0-9._=/+]/.test(core) || /[a-z][A-Z]/.test(core) || core.length >= 20
  );
}

function redactQueryValues(query: string): string {
  return query
    .split("&")
    .map((pair) => {
      const separator = pair.indexOf("=");
      if (separator === -1) return pair;
      const name = pair.slice(0, separator);
      return isSecretParamName(name) ? `${name}=[REDACTED]` : pair;
    })
    .join("&");
}

export function redactSensitiveText(value: string): string {
  return value
    .replace(
      URL_RE,
      (_match, scheme: string, rest: string, query: string) =>
        `${scheme}${rest}?${redactQueryValues(query)}`,
    )
    .replace(URL_USERINFO_RE, "$1[REDACTED]@")
    .replace(AUTH_HEADER_RE, "$1 [REDACTED]")
    .replace(AUTH_SCHEME_RE, (match, scheme: string, token: string) =>
      looksLikeSecret(token) ? `${scheme} [REDACTED]` : match,
    )
    .replace(SK_KEY_RE, "[REDACTED]")
    .replace(
      NAMED_SECRET_RE,
      (match, name: string, assignment: string, secret: string) =>
        /[=]/.test(assignment) ||
        /["']/.test(assignment) ||
        looksLikeSecret(secret)
          ? `${name}${assignment}[REDACTED]`
          : match,
    )
    .replace(EMAIL_RE, "[EMAIL_REDACTED]");
}

/**
 * The 512-character lid, for the message ONLY. A stack is long by nature and
 * loses its useful frames when cut; it goes through the patterns and nothing
 * else.
 */
export function capLoggedMessage(value: string): string {
  return value.length > MAX_LOGGED_MESSAGE_LENGTH
    ? `${value.slice(0, MAX_LOGGED_MESSAGE_LENGTH)}…`
    : value;
}

export function operationalErrorMetadata(error: unknown) {
  if (!error || typeof error !== "object") {
    return { type: "Error" };
  }

  const record = error as Record<string, unknown>;
  const statusValue = record.status ?? record.statusCode;
  const status =
    typeof statusValue === "number" &&
    Number.isInteger(statusValue) &&
    statusValue >= 100 &&
    statusValue <= 599
      ? statusValue
      : undefined;

  return {
    type: safeIdentifier(record.name) ?? "Error",
    code: safeIdentifier(record.code),
    stage: safeIdentifier(record.stage),
    status,
  };
}

export function normalizeOperationalError(
  error: unknown,
  operation: string,
): Error {
  if (error instanceof Error) return error;

  if (
    typeof error === "string" ||
    typeof error === "number" ||
    typeof error === "boolean"
  ) {
    return new Error(`${operation} failed: ${String(error).slice(0, 256)}`);
  }

  if (error && typeof error === "object") {
    const record = error as Record<string, unknown>;
    const details = ["status", "statusCode", "code", "message"]
      .flatMap((key) => {
        const value = record[key];
        return typeof value === "string" ||
          typeof value === "number" ||
          typeof value === "boolean"
          ? [`${key}=${String(value).slice(0, 256)}`]
          : [];
      })
      .join(", ");

    if (details) {
      return new Error(`${operation} failed (${details})`);
    }
  }

  return new Error(`${operation} failed`);
}

/**
 * Which `tags`/`context` keys may reach the log file. Measured on the code
 * base on 02.09.2026: no live caller passes tags or context at all; the two
 * keys that ever did (git log -S captureOperationalError) were `provider`
 * (tags) and `enabled` (context). Everything else -- transcript excerpts,
 * URLs, mail text -- has no business in a file the user forwards to others.
 * Keys are compared lower-cased. Values are checked too, and only primitives
 * pass: a boolean, a number, or a string that looks like an identifier. The
 * type says primitive, but an `as` cast at the call site says nothing (C2,
 * Review 02.09.2026) -- an object under an allowlisted key used to walk
 * through.
 */
const LOGGED_DETAIL_KEYS: ReadonlySet<string> = new Set([
  "provider",
  "enabled",
]);

type LoggedDetailValue = boolean | number | string;

function loggableDetailValue(value: unknown): LoggedDetailValue | undefined {
  if (typeof value === "boolean" || typeof value === "number") return value;
  if (typeof value === "string") return safeIdentifier(value);
  return undefined;
}

function loggableDetails(
  ...sources: Array<Record<string, ErrorContextValue> | undefined>
): Record<string, LoggedDetailValue> {
  const details: Record<string, LoggedDetailValue> = {};
  for (const source of sources) {
    for (const [rawKey, value] of Object.entries(source ?? {})) {
      const key = rawKey.toLowerCase();
      if (!LOGGED_DETAIL_KEYS.has(key)) continue;
      const loggable = loggableDetailValue(value);
      if (loggable === undefined) continue;
      details[key] = loggable;
    }
  }
  return details;
}

export function captureOperationalError(
  error: unknown,
  {
    operation,
    level = "error",
    tags,
    context,
  }: {
    operation: string;
    level?: SeverityLevel;
    tags?: Record<string, ErrorContextValue>;
    context?: Record<string, ErrorContextValue>;
  },
) {
  const metadata = operationalErrorMetadata(error);
  const exception = normalizeOperationalError(error, operation);
  const message = capLoggedMessage(redactSensitiveText(exception.message));
  // `message` travels as a plain string on purpose: the webview bridge
  // (plugins/tracing, JS_INIT_SCRIPT) JSON-serialises the console arguments,
  // and an Error object has no enumerable properties -- without this copy the
  // log file would show `{}` where the reason was. Every string in here goes
  // through the redaction, the allowlisted ones included: "sk-live-abc123"
  // passes the identifier check.
  const details = redactStrings({
    operation,
    message,
    ...metadata,
    ...loggableDetails(tags, context),
  });

  const write =
    level === "warning"
      ? console.warn
      : level === "log" || level === "info" || level === "debug"
        ? console.info
        : console.error;
  write(`[${operation}]`, loggableException(exception, message), details);
}

function redactStrings<T extends Record<string, ErrorContextValue | undefined>>(
  values: T,
): T {
  const out: Record<string, ErrorContextValue | undefined> = {};
  for (const [key, value] of Object.entries(values)) {
    out[key] = typeof value === "string" ? redactSensitiveText(value) : value;
  }
  return out as T;
}

/**
 * The Error handed to the console. `JSON.stringify(new Error("x"))` is `{}`,
 * which is why the plain-string copy above exists -- but a subclass may add
 * enumerable fields of its own (a response body, request options), and those
 * would cross the bridge untouched. The copy carries the redacted message,
 * keeps name, stack and cause for the devtools as non-enumerable properties
 * -- all three redacted, the name too (D1: a subclass may name itself after
 * what it carries), the stack uncapped (patterns only; cutting it costs the
 * frames that matter) -- and owns nothing enumerable.
 */
function loggableException(exception: Error, message: string): Error {
  const copy = new Error(message);
  // `Error.cause` is ES2022; the lib target here predates it.
  const cause = (exception as { cause?: unknown }).cause;
  const loggableCause =
    cause instanceof Error
      ? loggableException(
          cause,
          capLoggedMessage(redactSensitiveText(cause.message)),
        )
      : typeof cause === "string"
        ? redactSensitiveText(cause)
        : undefined;
  for (const [key, value] of [
    ["name", redactSensitiveText(exception.name)],
    ["stack", redactSensitiveText(exception.stack ?? "")],
    ...(loggableCause === undefined ? [] : [["cause", loggableCause] as const]),
  ] as const) {
    Object.defineProperty(copy, key, {
      value,
      enumerable: false,
      configurable: true,
      writable: true,
    });
  }
  return copy;
}
