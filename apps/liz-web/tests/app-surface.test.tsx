import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

const runtime = {
  connectionState: "idle",
  error: null,
  state: {
    threads: [],
    aboutYou: null,
    carrying: null,
    knowledge: null,
    nodes: [],
    workspaceMounts: [],
    providerProfiles: [],
    runtimeConfig: null,
    modelStatus: null,
  },
  activeThread: null,
  activeTranscript: [],
  activeRuntime: { activeTurnId: null, lastError: null },
  activeToolCalls: [],
  activeApprovals: [],
  allApprovals: [],
  activeMemory: null,
  activeResumePanel: null,
  selectedToolCall: null,
  selectToolCall: vi.fn(),
  connect: vi.fn(),
  close: vi.fn(),
  refreshThreads: vi.fn(),
  setActiveThread: vi.fn(),
  startThread: vi.fn(),
  forkThread: vi.fn(),
  startTurn: vi.fn(),
  cancelTurn: vi.fn(),
  respondToApproval: vi.fn(),
  readMemoryWakeup: vi.fn(),
  compileMemory: vi.fn(),
  listMemoryTopics: vi.fn(),
  searchMemory: vi.fn(),
  openMemoryEvidence: vi.fn(),
  loadRuntimeState: vi.fn(),
  loadOwnerSurfaces: vi.fn(),
  updateRuntimeConfig: vi.fn(),
  upsertProviderProfile: vi.fn(),
  deleteProviderProfile: vi.fn(),
};

vi.mock("../src/hooks/useLizRuntime", () => ({
  useLizRuntime: () => runtime,
}));

describe("App product surface", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it("uses Home as the default owner-facing surface", () => {
    render(<App />);

    const nav = screen.getByLabelText("Primary navigation");
    expect(within(nav).getByLabelText("Home")).toBeInTheDocument();
    expect(within(nav).getByLabelText("People")).toBeInTheDocument();
    expect(within(nav).getByLabelText("Channels")).toBeInTheDocument();
    expect(within(nav).getByLabelText("Devices")).toBeInTheDocument();
    expect(within(nav).getByLabelText("Workspaces")).toBeInTheDocument();
    expect(within(nav).getByLabelText("Settings")).toBeInTheDocument();

    expect(screen.getByRole("heading", { name: "Home" })).toBeInTheDocument();
    expect(screen.queryByText("Liz Console")).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Memory" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Approvals" })).not.toBeInTheDocument();
    expect(screen.queryByText("Wakeup")).not.toBeInTheDocument();
    expect(screen.queryByText("Compile now")).not.toBeInTheDocument();
    expect(screen.queryByText("Inspector")).not.toBeInTheDocument();
  });
});
