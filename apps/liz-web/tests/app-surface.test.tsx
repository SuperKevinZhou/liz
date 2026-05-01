import { fireEvent, render, screen, within } from "@testing-library/react";
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
    people: null,
    nodes: [],
    workspaceMounts: [],
    providerProfiles: [],
    runtimeConfig: null,
    modelStatus: null,
    memoryTopics: [],
    memorySearch: null,
    selectedEvidence: null,
  },
  activeThread: null,
  activeTranscript: [],
  activeRuntime: { activeTurnId: null, lastError: null },
  activeToolCalls: [],
  activeApprovals: [],
  allApprovals: [],
  activeMemory: { wakeup: null, recentConversation: null, compilation: null },
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
  loadPeopleSurface: vi.fn(),
  upsertPersonBoundary: vi.fn(),
  deletePersonBoundary: vi.fn(),
  upsertProviderProfile: vi.fn(),
  deleteProviderProfile: vi.fn(),
  attachWorkspaceMount: vi.fn(),
  detachWorkspaceMount: vi.fn(),
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
    expect(within(nav).queryByLabelText("Diagnostics")).not.toBeInTheDocument();

    expect(screen.getByRole("heading", { name: "Home" })).toBeInTheDocument();
    expect(screen.queryByText("Liz Console")).not.toBeInTheDocument();
    expect(screen.queryByText("Workspace path optional")).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Memory" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Approvals" })).not.toBeInTheDocument();
    expect(screen.queryByText("Wakeup")).not.toBeInTheDocument();
    expect(screen.queryByText("Compile now")).not.toBeInTheDocument();
    expect(screen.queryByText("Inspector")).not.toBeInTheDocument();
  });

  it("reveals Diagnostics only after developer mode is enabled", () => {
    render(<App />);

    const nav = screen.getByLabelText("Primary navigation");
    expect(within(nav).queryByLabelText("Diagnostics")).not.toBeInTheDocument();

    fireEvent.click(within(nav).getByLabelText("Settings"));
    fireEvent.click(screen.getByLabelText("Enable Diagnostics"));

    expect(within(nav).getByLabelText("Diagnostics")).toBeInTheDocument();
    fireEvent.click(within(nav).getByLabelText("Diagnostics"));
    expect(screen.getByRole("heading", { name: "Memory" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Approvals" })).toBeInTheDocument();
  });
});
