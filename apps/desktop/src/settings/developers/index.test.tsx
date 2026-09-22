import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  checkEmbeddedCli: vi.fn(),
  installEmbeddedCli: vi.fn(),
  listSkillAgents: vi.fn(),
  installAgentSkill: vi.fn(),
  showDevtool: vi.fn(),
  devtoolsPanelShow: vi.fn(),
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
  openUrl: vi.fn(),
  getCloudApiSettings: vi.fn(),
  setCloudApiEnabled: vi.fn(),
  backfillCloudApiSnapshots: vi.fn(),
  createCloudApiKey: vi.fn(),
  billing: {
    isPro: true,
    isReady: true,
    isUpgradingToPro: false,
    upgradeToPro: vi.fn(),
  },
}));

vi.mock("~/types/tauri.gen", () => ({
  commands: {
    checkEmbeddedCli: mocks.checkEmbeddedCli,
    installEmbeddedCli: mocks.installEmbeddedCli,
    listSkillAgents: mocks.listSkillAgents,
    installAgentSkill: mocks.installAgentSkill,
    showDevtool: mocks.showDevtool,
  },
}));

vi.mock("@anlg/ui/components/ui/dropdown-menu", () => ({
  AppFloatingPanel: ({ children }: { children: React.ReactNode }) => (
    <div>{children}</div>
  ),
  DropdownMenu: ({ children }: { children: React.ReactNode }) => (
    <div>{children}</div>
  ),
  DropdownMenuContent: ({ children }: { children: React.ReactNode }) => (
    <div>{children}</div>
  ),
  DropdownMenuItem: ({
    children,
    ...props
  }: React.ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button type="button" {...props}>
      {children}
    </button>
  ),
  DropdownMenuSeparator: () => <hr />,
  DropdownMenuTrigger: ({ children }: { children: React.ReactNode }) => (
    <>{children}</>
  ),
}));

vi.mock("@anlg/plugin-windows", () => ({
  commands: {
    devtoolsPanelShow: mocks.devtoolsPanelShow,
  },
}));

vi.mock("@anlg/plugin-opener2", () => ({
  commands: { openUrl: mocks.openUrl },
}));

vi.mock("@anlg/plugin-local-api", () => ({
  commands: {
    listWebhooks: vi.fn().mockResolvedValue({ status: "ok", data: [] }),
  },
}));

vi.mock("@anlg/ui/components/ui/toast", () => ({
  sonnerToast: { error: mocks.toastError, success: mocks.toastSuccess },
}));

import {
  SettingsDevelopers,
  buildMcpConfiguration,
  getCliInstallNotification,
} from "./index";

describe("buildMcpConfiguration", () => {
  it("uses the exact installed CLI path", () => {
    const configuration = JSON.parse(
      buildMcpConfiguration("/Users/test/.local/bin/anarlog"),
    );

    expect(configuration).toEqual({
      mcpServers: {
        mitschnitt: {
          command: "/Users/test/.local/bin/anarlog",
          args: ["mcp"],
        },
      },
    });
  });
});

describe("getCliInstallNotification", () => {
  it("reports installed as success", () => {
    expect(
      getCliInstallNotification({
        supported: true,
        commandName: "anarlog",
        installPath: "/Users/test/.local/bin/anarlog",
        state: "installed",
        details: "Installed.",
      }),
    ).toEqual({ type: "success", message: "anarlog is ready to use" });
  });

  it.each(["resource_missing", "unsupported"] as const)(
    "reports %s as an install error",
    (state) => {
      expect(
        getCliInstallNotification({
          supported: false,
          commandName: "anarlog",
          installPath: "/Users/test/.local/bin/anarlog",
          state,
          details: "The CLI is unavailable in this build.",
        }),
      ).toEqual({
        type: "error",
        message: "The CLI is unavailable in this build.",
      });
    },
  );
});

describe("SettingsDevelopers", () => {
  beforeEach(() => {
    mocks.checkEmbeddedCli.mockReset();
    mocks.installEmbeddedCli.mockReset();
    mocks.listSkillAgents.mockReset();
    mocks.listSkillAgents.mockResolvedValue({ status: "ok", data: [] });
    mocks.installAgentSkill.mockReset();
    mocks.showDevtool.mockReset();
    mocks.showDevtool.mockResolvedValue(false);
    mocks.devtoolsPanelShow.mockReset();
    mocks.devtoolsPanelShow.mockResolvedValue({ status: "ok" });
    mocks.toastError.mockReset();
    mocks.toastSuccess.mockReset();
    mocks.openUrl.mockReset();
    mocks.createCloudApiKey.mockReset();
    mocks.getCloudApiSettings.mockReset();
    mocks.getCloudApiSettings.mockResolvedValue({
      enabled: false,
      updated_at: null,
    });
    mocks.setCloudApiEnabled.mockReset();
    mocks.backfillCloudApiSnapshots.mockReset();
    mocks.billing.isPro = true;
    mocks.billing.isReady = true;
    mocks.billing.isUpgradingToPro = false;
    mocks.billing.upgradeToPro.mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  it("uses the installed CLI path when copying the MCP configuration", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    mocks.checkEmbeddedCli.mockResolvedValue({
      status: "ok",
      data: {
        supported: true,
        commandName: "anarlog",
        installPath: "/Users/test/.local/bin/anarlog",
        state: "installed",
        details:
          "Installed at /Users/test/.local/bin/anarlog and managed by Mitschnitt.",
      },
    });

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={queryClient}>
        <SettingsDevelopers />
      </QueryClientProvider>,
    );

    expect(await screen.findByText("Reinstall")).toBeTruthy();
    expect(screen.getByLabelText("Installed")).toBeTruthy();
    expect(screen.queryByText("Installed")).toBeNull();
    expect(
      screen.queryByText(/\/Users\/test\/\.local\/bin\/anarlog/),
    ).toBeNull();
    expect(screen.queryByText("anarlog --json meetings list")).toBeNull();
    expect(screen.queryByText("anarlog mcp")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Copy config" }));

    await waitFor(() => expect(writeText).toHaveBeenCalledOnce());
    expect(JSON.parse(writeText.mock.calls[0][0])).toEqual({
      mcpServers: {
        mitschnitt: {
          command: "/Users/test/.local/bin/anarlog",
          args: ["mcp"],
        },
      },
    });
  });

  it("does not expose a nonexistent MCP path when the CLI is unsupported", async () => {
    mocks.checkEmbeddedCli.mockResolvedValue({
      status: "ok",
      data: {
        supported: false,
        commandName: "anarlog-dev",
        installPath: "/Users/test/.local/bin/anarlog-dev",
        state: "unsupported",
        details: "Bundled CLI installation is currently available on macOS.",
      },
    });

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={queryClient}>
        <SettingsDevelopers />
      </QueryClientProvider>,
    );

    const copyButton = await screen.findByRole("button", {
      name: "Copy config",
    });
    expect(copyButton.hasAttribute("disabled")).toBe(true);
    expect(
      screen.queryByText(/\/Users\/test\/\.local\/bin\/anarlog-dev/),
    ).toBeNull();
  });

  it("installs the skill into every detected agent from one action", async () => {
    mocks.checkEmbeddedCli.mockResolvedValue({
      status: "ok",
      data: {
        supported: false,
        commandName: "anarlog",
        installPath: "/Users/test/.local/bin/anarlog",
        state: "unsupported",
        details: "Unavailable.",
      },
    });
    mocks.listSkillAgents.mockResolvedValue({
      status: "ok",
      data: [
        {
          agent: "claude_code",
          displayName: "Claude Code",
          detected: true,
          installed: true,
          skillPath: "/Users/test/.claude/skills/mitschnitt",
        },
        {
          agent: "codex",
          displayName: "Codex",
          detected: true,
          installed: false,
          skillPath: "/Users/test/.codex/skills/mitschnitt",
        },
        {
          agent: "cursor",
          displayName: "Cursor",
          detected: false,
          installed: false,
          skillPath: "/Users/test/.cursor/skills/mitschnitt",
        },
        {
          agent: "opencode",
          displayName: "OpenCode",
          detected: true,
          installed: false,
          skillPath: "/Users/test/.config/opencode/skills/mitschnitt",
        },
      ],
    });
    mocks.installAgentSkill.mockImplementation((agent: string) =>
      Promise.resolve({
        status: "ok",
        data: {
          agent,
          displayName: agent,
          detected: true,
          installed: true,
          skillPath: `/Users/test/${agent}`,
        },
      }),
    );

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={queryClient}>
        <SettingsDevelopers />
      </QueryClientProvider>,
    );

    const cursorItem = await screen.findByRole("button", { name: "Cursor" });
    expect(cursorItem.hasAttribute("disabled")).toBe(true);
    const installedIcon = screen.getByLabelText("Skill installed");
    expect(installedIcon.closest("button")?.textContent).toContain(
      "Claude Code",
    );

    fireEvent.click(
      screen.getByRole("button", { name: "Install to all agents" }),
    );

    await waitFor(() =>
      expect(mocks.installAgentSkill).toHaveBeenCalledTimes(3),
    );
    expect(mocks.installAgentSkill).toHaveBeenCalledWith("claude_code");
    expect(mocks.installAgentSkill).toHaveBeenCalledWith("codex");
    expect(mocks.installAgentSkill).toHaveBeenCalledWith("opencode");
    expect(mocks.installAgentSkill).not.toHaveBeenCalledWith("cursor");
    await waitFor(() =>
      expect(mocks.toastSuccess).toHaveBeenCalledWith(
        "Mitschnitt skill added to 3 agents",
      ),
    );
  });

  it("reports a single-agent skill install by agent name", async () => {
    mocks.checkEmbeddedCli.mockResolvedValue({
      status: "ok",
      data: {
        supported: false,
        commandName: "anarlog",
        installPath: "/Users/test/.local/bin/anarlog",
        state: "unsupported",
        details: "Unavailable.",
      },
    });
    mocks.listSkillAgents.mockResolvedValue({
      status: "ok",
      data: [
        {
          agent: "codex",
          displayName: "Codex",
          detected: true,
          installed: false,
          skillPath: "/Users/test/.codex/skills/mitschnitt",
        },
      ],
    });
    mocks.installAgentSkill.mockResolvedValue({
      status: "ok",
      data: {
        agent: "codex",
        displayName: "Codex",
        detected: true,
        installed: true,
        skillPath: "/Users/test/.codex/skills/mitschnitt",
      },
    });

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={queryClient}>
        <SettingsDevelopers />
      </QueryClientProvider>,
    );

    fireEvent.click(await screen.findByRole("button", { name: "Codex" }));

    await waitFor(() =>
      expect(mocks.installAgentSkill).toHaveBeenCalledWith("codex"),
    );
    await waitFor(() =>
      expect(mocks.toastSuccess).toHaveBeenCalledWith(
        "Mitschnitt skill added to Codex",
      ),
    );
  });

  it("hides the devtools section when devtools are disabled", async () => {
    mocks.checkEmbeddedCli.mockResolvedValue({
      status: "ok",
      data: {
        supported: false,
        commandName: "anarlog",
        installPath: "/Users/test/.local/bin/anarlog",
        state: "unsupported",
        details: "Unavailable.",
      },
    });

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={queryClient}>
        <SettingsDevelopers />
      </QueryClientProvider>,
    );

    await waitFor(() => expect(mocks.showDevtool).toHaveBeenCalled());
    expect(screen.queryByText("Devtools panel")).toBeNull();
  });

  it("opens the devtools panel from the settings button when enabled", async () => {
    mocks.checkEmbeddedCli.mockResolvedValue({
      status: "ok",
      data: {
        supported: false,
        commandName: "anarlog",
        installPath: "/Users/test/.local/bin/anarlog",
        state: "unsupported",
        details: "Unavailable.",
      },
    });
    mocks.showDevtool.mockResolvedValue(true);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={queryClient}>
        <SettingsDevelopers />
      </QueryClientProvider>,
    );

    expect(await screen.findByText("Devtools panel")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Open panel" }));

    await waitFor(() =>
      expect(mocks.devtoolsPanelShow).toHaveBeenCalledTimes(1),
    );
  });
});
