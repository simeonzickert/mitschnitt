import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  selectFile: vi.fn(),
  selectTarget: vi.fn(),
  exportSettingsBundle: vi.fn(),
  inspectSettingsBundle: vi.fn(),
  readSettingsBundle: vi.fn(),
  collectBundlePayload: vi.fn(),
  planImport: vi.fn(),
  applyImport: vi.fn(),
  checkSelectedLocalModel: vi.fn(),
  handleDownload: vi.fn(),
  // A stand-in for the error class rather than the real module: importing the
  // real `./bundle` here would drag in the database handle this file has no
  // business starting. The class itself is exercised for real in
  // bundle.test.ts; what matters here is that the component and the thrower
  // agree on ONE class, which they do, because the component imports it from
  // the same mock. Declared inside `vi.hoisted` so it exists before the mock
  // factory runs.
  PartialImportError: class PartialImportError extends Error {
    constructor(
      readonly stage: string,
      readonly applied: string[],
      readonly pending: string[],
      readonly cause: unknown,
      readonly appliedItems: string[] = [],
    ) {
      super(`settings import stopped in stage "${stage}"`);
      this.name = "PartialImportError";
    }
  },
}));

vi.mock("@lingui/core/macro", () => ({
  // Interpolates eagerly. The real `msg` keeps placeholders and values apart;
  // for a test that only reads the finished sentence this is equivalent, and
  // without it every message with a value collapses to its literal halves.
  msg: (strings: TemplateStringsArray, ...values: unknown[]) => ({
    message: strings.reduce(
      (text, part, index) => text + String(values[index - 1] ?? "") + part,
    ),
  }),
}));

vi.mock("@lingui/react/macro", () => ({
  Trans: ({ children }: { children?: ReactNode }) => <>{children}</>,
  useLingui: () => ({
    i18n: { _: (descriptor: { message?: string }) => descriptor.message ?? "" },
    t: (input: TemplateStringsArray | string) =>
      typeof input === "string" ? input : (input as readonly string[]).join(""),
  }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: mocks.selectFile,
  save: mocks.selectTarget,
}));

vi.mock("~/types/tauri.gen", () => ({
  commands: {
    exportSettingsBundle: mocks.exportSettingsBundle,
    inspectSettingsBundle: mocks.inspectSettingsBundle,
    readSettingsBundle: mocks.readSettingsBundle,
  },
}));

vi.mock("./bundle", () => ({
  PartialImportError: mocks.PartialImportError,
  collectBundlePayload: mocks.collectBundlePayload,
  planImport: mocks.planImport,
  applyImport: mocks.applyImport,
  parseBundlePayload: (value: unknown) => value,
}));

vi.mock("./model", () => ({
  checkSelectedLocalModel: mocks.checkSelectedLocalModel,
}));

vi.mock("~/stt/useLocalSttModel", () => ({
  useLocalModelDownload: () => ({
    progress: 0,
    hasError: false,
    errorMessage: null,
    isDownloaded: false,
    isDownloadedLoading: false,
    showProgress: false,
    handleDownload: mocks.handleDownload,
    handleCancel: vi.fn(),
    handleDelete: vi.fn(),
  }),
}));

import { SettingsTransfer } from "./index";

const EMPTY_PLAN = {
  settings: [],
  providers: [],
  templates: [],
  unchangedSettings: 0,
  missingSelectedTemplateId: null,
};

const PLAN_WITH_A_FOREIGN_ADDRESS = {
  settings: [],
  providers: [
    {
      type: "llm",
      providerId: "openai",
      carriesKey: false,
      action: "update",
      baseUrl: "https://angreifer.tld/v1",
      isDefaultEndpoint: false,
      sendsExistingKeyElsewhere: true,
    },
  ],
  templates: [],
  unchangedSettings: 0,
  missingSelectedTemplateId: null,
};

const PLAN_WITH_A_KEY = {
  settings: [{ key: "theme", before: "light", after: "dark" }],
  providers: [
    {
      type: "stt",
      providerId: "openai",
      carriesKey: true,
      action: "add",
      baseUrl: "https://api.openai.com/v1",
      isDefaultEndpoint: true,
      sendsExistingKeyElsewhere: false,
    },
  ],
  templates: [{ id: "tpl-1", title: "Kundentermin", action: "add" }],
  unchangedSettings: 2,
  missingSelectedTemplateId: null,
};

const SEALED_INFO = {
  version: 1,
  created_at: "2026-08-31T10:00:00Z",
  app_version: "1.4.14",
  includes_secrets: true,
  encrypted: true,
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.collectBundlePayload.mockResolvedValue({ settings: {} });
  mocks.exportSettingsBundle.mockResolvedValue({ status: "ok", data: null });
  mocks.planImport.mockResolvedValue(EMPTY_PLAN);
  mocks.applyImport.mockResolvedValue(undefined);
  mocks.checkSelectedLocalModel.mockResolvedValue({ state: "not-applicable" });
});

afterEach(cleanup);

describe("exporting", () => {
  it("refuses to write a file with credentials and no password", async () => {
    render(<SettingsTransfer />);

    fireEvent.click(screen.getByRole("switch"));
    fireEvent.click(screen.getByRole("button", { name: /Save file/ }));

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain(
        "Please enter a password.",
      );
    });
    expect(mocks.selectTarget).not.toHaveBeenCalled();
    expect(mocks.exportSettingsBundle).not.toHaveBeenCalled();
  });

  it("refuses when the two passwords differ", async () => {
    render(<SettingsTransfer />);

    fireEvent.click(screen.getByRole("switch"));
    fireEvent.change(screen.getByLabelText("Password"), {
      target: { value: "einseins" },
    });
    fireEvent.change(screen.getByLabelText("Repeat password"), {
      target: { value: "zweizwei" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Save file/ }));

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain(
        "The two passwords are not the same.",
      );
    });
    expect(mocks.exportSettingsBundle).not.toHaveBeenCalled();
  });

  it("writes a shareable file without a password when credentials stay home", async () => {
    mocks.selectTarget.mockResolvedValue("/tmp/setup.json");
    render(<SettingsTransfer />);

    fireEvent.click(screen.getByRole("button", { name: /Save file/ }));

    await waitFor(() => {
      expect(mocks.exportSettingsBundle).toHaveBeenCalledWith(
        "/tmp/setup.json",
        expect.any(String),
        false,
        null,
      );
    });
    expect(mocks.collectBundlePayload).toHaveBeenCalledWith({
      includeSecrets: false,
    });
  });

  it("passes the password on when credentials travel", async () => {
    mocks.selectTarget.mockResolvedValue("/tmp/setup.json");
    render(<SettingsTransfer />);

    fireEvent.click(screen.getByRole("switch"));
    for (const label of ["Password", "Repeat password"]) {
      fireEvent.change(screen.getByLabelText(label), {
        target: { value: "geheim" },
      });
    }
    fireEvent.click(screen.getByRole("button", { name: /Save file/ }));

    await waitFor(() => {
      expect(mocks.exportSettingsBundle).toHaveBeenCalledWith(
        "/tmp/setup.json",
        expect.any(String),
        true,
        "geheim",
      );
    });
    expect(mocks.collectBundlePayload).toHaveBeenCalledWith({
      includeSecrets: true,
    });
  });
});

/** Chooses an unencrypted bundle and waits until its preview is on screen. */
async function openPlainBundle(plan: unknown) {
  mocks.selectFile.mockResolvedValue("/tmp/setup.json");
  mocks.inspectSettingsBundle.mockResolvedValue({
    status: "ok",
    data: { ...SEALED_INFO, encrypted: false, includes_secrets: false },
  });
  mocks.readSettingsBundle.mockResolvedValue({
    status: "ok",
    data: {
      info: { ...SEALED_INFO, encrypted: false },
      payload: JSON.stringify({ settings: {} }),
    },
  });
  mocks.planImport.mockResolvedValue(plan);

  render(<SettingsTransfer />);
  fireEvent.click(screen.getByRole("button", { name: /Choose file/ }));
  await waitFor(() => screen.getByRole("button", { name: /Apply/ }));
}

describe("importing", () => {
  it("asks for the password before reading a sealed file", async () => {
    mocks.selectFile.mockResolvedValue("/tmp/setup.json");
    mocks.inspectSettingsBundle.mockResolvedValue({
      status: "ok",
      data: SEALED_INFO,
    });

    render(<SettingsTransfer />);
    fireEvent.click(screen.getByRole("button", { name: /Choose file/ }));

    await waitFor(() => {
      expect(screen.getByLabelText("Password")).toBeTruthy();
    });
    expect(mocks.readSettingsBundle).not.toHaveBeenCalled();
  });

  it("says so on a wrong password and imports nothing", async () => {
    mocks.selectFile.mockResolvedValue("/tmp/setup.json");
    mocks.inspectSettingsBundle.mockResolvedValue({
      status: "ok",
      data: SEALED_INFO,
    });
    mocks.readSettingsBundle.mockResolvedValue({
      status: "error",
      error: "wrong_password",
    });

    render(<SettingsTransfer />);
    fireEvent.click(screen.getByRole("button", { name: /Choose file/ }));
    await waitFor(() => screen.getByLabelText("Password"));
    fireEvent.change(screen.getByLabelText("Password"), {
      target: { value: "falsch" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Open$/ }));

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain(
        "The password does not fit this file. Nothing was imported.",
      );
    });
    expect(mocks.applyImport).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: /Apply/ })).toBeNull();
  });

  it("shows what would change and writes only after Apply", async () => {
    mocks.selectFile.mockResolvedValue("/tmp/setup.json");
    mocks.inspectSettingsBundle.mockResolvedValue({
      status: "ok",
      data: { ...SEALED_INFO, encrypted: false, includes_secrets: false },
    });
    mocks.readSettingsBundle.mockResolvedValue({
      status: "ok",
      data: {
        info: { ...SEALED_INFO, encrypted: false },
        payload: JSON.stringify({ settings: {} }),
      },
    });
    mocks.planImport.mockResolvedValue(PLAN_WITH_A_KEY);

    render(<SettingsTransfer />);
    fireEvent.click(screen.getByRole("button", { name: /Choose file/ }));

    await waitFor(() => {
      expect(screen.getByTestId("change-theme").textContent).toContain(
        "theme: light -> dark",
      );
    });
    expect(screen.getByText(/Kundentermin/)).toBeTruthy();
    expect(mocks.applyImport).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: /Apply/ }));
    await waitFor(() => {
      expect(mocks.applyImport).toHaveBeenCalledTimes(1);
    });
  });

  it("offers the download when the imported setup needs a model that is missing", async () => {
    mocks.selectFile.mockResolvedValue("/tmp/setup.json");
    mocks.inspectSettingsBundle.mockResolvedValue({
      status: "ok",
      data: { ...SEALED_INFO, encrypted: false },
    });
    mocks.readSettingsBundle.mockResolvedValue({
      status: "ok",
      data: {
        info: { ...SEALED_INFO, encrypted: false },
        payload: JSON.stringify({ settings: {} }),
      },
    });
    mocks.checkSelectedLocalModel.mockResolvedValue({
      state: "missing",
      model: "QuantizedLargeV3Turbo",
    });

    render(<SettingsTransfer />);
    fireEvent.click(screen.getByRole("button", { name: /Choose file/ }));
    await waitFor(() => screen.getByRole("button", { name: /Apply/ }));
    fireEvent.click(screen.getByRole("button", { name: /Apply/ }));

    await waitFor(() => {
      expect(screen.getByTestId("missing-model-notice")).toBeTruthy();
    });
    fireEvent.click(screen.getByRole("button", { name: /Download model/ }));
    expect(mocks.handleDownload).toHaveBeenCalledTimes(1);
  });

  it("tells the user to pick their own model file again, and offers no download", async () => {
    mocks.selectFile.mockResolvedValue("/tmp/setup.json");
    mocks.inspectSettingsBundle.mockResolvedValue({
      status: "ok",
      data: { ...SEALED_INFO, encrypted: false },
    });
    mocks.readSettingsBundle.mockResolvedValue({
      status: "ok",
      data: {
        info: { ...SEALED_INFO, encrypted: false },
        payload: JSON.stringify({ settings: {} }),
      },
    });
    mocks.checkSelectedLocalModel.mockResolvedValue({ state: "local-file" });

    render(<SettingsTransfer />);
    fireEvent.click(screen.getByRole("button", { name: /Choose file/ }));
    await waitFor(() => screen.getByRole("button", { name: /Apply/ }));
    fireEvent.click(screen.getByRole("button", { name: /Apply/ }));

    await waitFor(() => {
      expect(screen.getByTestId("local-file-model-notice")).toBeTruthy();
    });
    // There is no path to check, and nothing on any server to fetch for it --
    // a download button here would promise something that does not exist.
    expect(screen.queryByRole("button", { name: /Download model/ })).toBeNull();
    expect(screen.queryByTestId("missing-model-notice")).toBeNull();
  });

  it("admits when it could not check the model instead of claiming it is fine", async () => {
    mocks.selectFile.mockResolvedValue("/tmp/setup.json");
    mocks.inspectSettingsBundle.mockResolvedValue({
      status: "ok",
      data: { ...SEALED_INFO, encrypted: false },
    });
    mocks.readSettingsBundle.mockResolvedValue({
      status: "ok",
      data: {
        info: { ...SEALED_INFO, encrypted: false },
        payload: JSON.stringify({ settings: {} }),
      },
    });
    mocks.checkSelectedLocalModel.mockResolvedValue({
      state: "unknown",
      reason: "model directory unreadable",
    });

    render(<SettingsTransfer />);
    fireEvent.click(screen.getByRole("button", { name: /Choose file/ }));
    await waitFor(() => screen.getByRole("button", { name: /Apply/ }));
    fireEvent.click(screen.getByRole("button", { name: /Apply/ }));

    await waitFor(() => {
      expect(
        screen.getByText(/could not check whether the transcription model/),
      ).toBeTruthy();
    });
    expect(screen.queryByTestId("missing-model-notice")).toBeNull();
  });

  // K5. The setting is applied faithfully, so nothing is broken -- but the
  // summary would quietly fall back to the default, and a preview that shows
  // only the settings line does not tell anybody that.
  it("warns when the file selects a template that is nowhere to be found", async () => {
    await openPlainBundle({
      ...EMPTY_PLAN,
      settings: [
        { key: "selected_template_id", before: undefined, after: "tpl-weg" },
      ],
      missingSelectedTemplateId: "tpl-weg",
    });

    expect(screen.getByTestId("missing-template-hint").textContent).toContain(
      "neither in it nor on this machine",
    );
  });

  it("stays quiet when the selected template is there", async () => {
    await openPlainBundle(PLAN_WITH_A_KEY);

    expect(screen.queryByTestId("missing-template-hint")).toBeNull();
  });

  // E2. Three stores, no shared transaction. When it stops partway the message
  // has to say what landed -- before this it printed the raw thrown text
  // ("keychain locked"), untranslated and useless for deciding what to do.
  it("says which stages landed when the import stops partway", async () => {
    await openPlainBundle(PLAN_WITH_A_KEY);

    mocks.applyImport.mockRejectedValueOnce(
      new mocks.PartialImportError(
        "providers",
        ["templates"],
        ["providers", "settings"],
        new Error("keychain locked"),
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: /Apply/ }));

    await waitFor(() => {
      const text = screen.getByRole("alert").textContent ?? "";
      expect(text).toContain("The import could not be completed.");
      expect(text).toContain("Already applied: summary templates.");
      expect(text).toContain("Not applied: providers, settings.");
      expect(text).toContain("Running the import again is safe");
      // The raw cause must not reach the user.
      expect(text).not.toContain("keychain locked");
    });

    // The preview and the Apply button stay, so a second run is one click away.
    expect(screen.getByRole("button", { name: /Apply/ })).toBeTruthy();
  });

  // B2. A stage that writes one item at a time can itself fail partway
  // through -- the failing stage still shows up under "Not applied" (it did
  // not finish), but a template that already reached the database must not
  // be reported as if nothing happened.
  it("names an item already written when a stage stops on its second item", async () => {
    await openPlainBundle(PLAN_WITH_A_KEY);

    mocks.applyImport.mockRejectedValueOnce(
      new mocks.PartialImportError(
        "templates",
        [],
        ["templates", "providers", "settings"],
        new Error("disk full"),
        ["Kundentermin"],
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: /Apply/ }));

    await waitFor(() => {
      const text = screen.getByRole("alert").textContent ?? "";
      expect(text).toContain("The import could not be completed.");
      expect(text).toContain(
        "Already wrote part of summary templates: Kundentermin.",
      );
      expect(text).toContain(
        "Not applied: summary templates, providers, settings.",
      );
      // The stage is still pending overall, but it must not read as if
      // nothing at all happened -- the template above already contradicts
      // that.
      expect(text).not.toContain("Nothing was applied yet.");
      // The raw cause must not reach the user.
      expect(text).not.toContain("disk full");
    });
  });

  // H10. The decrypted payload carries every imported API key. Once it has been
  // written there is no reason to keep it in component state for as long as the
  // settings tab stays open.
  it("lets go of the decrypted payload once it has been applied", async () => {
    await openPlainBundle(PLAN_WITH_A_KEY);

    fireEvent.click(screen.getByRole("button", { name: /Apply/ }));
    const confirmation = await waitFor(() =>
      screen.getByText(/The setup has been applied/),
    );

    // Deliberately NOT asserted through the preview being gone: that is driven
    // by `applied`, so it disappears either way and the assertion would be
    // green on the broken version too.
    expect(confirmation.parentElement?.dataset.payloadHeld).toBe("false");
  });

  // The counterpart: while an import is still pending the payload IS held, so
  // the marker above is reading real state rather than a constant.
  it("still holds the payload while the import has not run", async () => {
    await openPlainBundle(PLAN_WITH_A_KEY);

    mocks.applyImport.mockRejectedValueOnce(
      new mocks.PartialImportError("providers", [], [], new Error("nope")),
    );
    fireEvent.click(screen.getByRole("button", { name: /Apply/ }));

    await waitFor(() => screen.getByRole("alert"));
    // A failed import keeps the payload, which is what makes a retry possible.
    expect(screen.getByRole("button", { name: /Apply/ })).toBeTruthy();
  });

  it("names a file that is not a bundle at all", async () => {
    mocks.selectFile.mockResolvedValue("/tmp/urlaubsfoto.json");
    mocks.inspectSettingsBundle.mockResolvedValue({
      status: "error",
      error: "not_a_bundle",
    });

    render(<SettingsTransfer />);
    fireEvent.click(screen.getByRole("button", { name: /Choose file/ }));

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain(
        "This file is not a Mitschnitt settings file.",
      );
    });
    expect(mocks.readSettingsBundle).not.toHaveBeenCalled();
  });
});

// Mitschnitt-Fork (F14). A bundle needs no credential to do harm: it points a
// provider at a host of the writer's choosing, the importing machine keeps its
// own key exactly as intended, and from then on that key and the contents of
// every meeting go to whoever wrote the file. The preview used to show only
// "with key / without key", so there was nothing to notice.
describe("importing a file that points somewhere else", () => {
  it("shows the address and says what would be sent there", async () => {
    await openPlainBundle(PLAN_WITH_A_FOREIGN_ADDRESS);

    expect(screen.getByText("https://angreifer.tld/v1")).toBeTruthy();
    expect(screen.getByText(/own address/i).textContent).toContain(
      "access key",
    );
  });

  it("does not apply until the person says they know", async () => {
    await openPlainBundle(PLAN_WITH_A_FOREIGN_ADDRESS);

    const apply = screen.getByRole("button", { name: /Apply/ });
    expect(apply.hasAttribute("disabled")).toBe(true);
    fireEvent.click(apply);
    expect(mocks.applyImport).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: /Apply/ }));

    await waitFor(() => {
      expect(mocks.applyImport).toHaveBeenCalledWith(expect.anything(), {
        confirmedEndpoints: true,
      });
    });
  });

  // The ordinary bundle must not grow a click: a confirmation everybody clicks
  // past is a confirmation nobody reads.
  it("asks nothing extra when the address is the provider's own", async () => {
    await openPlainBundle(PLAN_WITH_A_KEY);

    expect(
      screen.getByRole("button", { name: /Apply/ }).hasAttribute("disabled"),
    ).toBe(false);
    expect(screen.queryByRole("checkbox")).toBeNull();
  });
});
