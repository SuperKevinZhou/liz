import {
  Bot,
  Brain,
  CircleDot,
  Command,
  FolderKanban,
  GitFork,
  HardDrive,
  MessageSquareText,
  PlugZap,
  Search,
  Send,
  Settings,
  ShieldCheck,
  Sparkles,
  Square,
  TerminalSquare,
  Trash2,
  UserRound,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useLizRuntime, type LizRuntime } from "./hooks/useLizRuntime";
import { loadPreferences, savePreferences, type Preferences } from "./preferences";
import type { KnowledgeItem, PersonBoundary, Thread } from "./protocol/types";
import type { TranscriptEntry } from "./state/workbench";

type ViewId = "home" | "people" | "channels" | "devices" | "workspaces" | "settings" | "diagnostics";

const primaryViews: Array<{ id: Exclude<ViewId, "diagnostics">; label: string; icon: React.ComponentType<{ size?: number }> }> = [
  { id: "home", label: "Home", icon: MessageSquareText },
  { id: "people", label: "People", icon: UserRound },
  { id: "channels", label: "Channels", icon: PlugZap },
  { id: "devices", label: "Devices", icon: HardDrive },
  { id: "workspaces", label: "Workspaces", icon: FolderKanban },
  { id: "settings", label: "Settings", icon: Settings },
];

export function App() {
  const [activeView, setActiveView] = useState<ViewId>("home");
  const [preferences, setPreferences] = useState<Preferences>(() => loadPreferences());
  const runtime = useLizRuntime(preferences);

  useEffect(() => {
    savePreferences(preferences);
  }, [preferences]);

  const visibleActiveView =
    activeView === "diagnostics" && !preferences.developerMode ? "home" : activeView;

  const shellClassName = useMemo(
    () =>
      `liz-shell view-${visibleActiveView} density-${preferences.density} theme-${preferences.theme}`,
    [visibleActiveView, preferences.density, preferences.theme],
  );

  return (
    <div className={shellClassName}>
      <aside className="rail" aria-label="Primary navigation">
        <div className="mark" aria-label="Liz">
          <Sparkles size={18} />
        </div>
        <nav>
          {primaryViews.map((view) => {
            const Icon = view.icon;
            return (
              <button
                key={view.id}
                className={visibleActiveView === view.id ? "rail-button active" : "rail-button"}
                onClick={() => setActiveView(view.id)}
                title={view.label}
                aria-label={view.label}
              >
                <Icon size={18} />
              </button>
            );
          })}
          {preferences.developerMode ? (
            <button
              className={visibleActiveView === "diagnostics" ? "rail-button active" : "rail-button"}
              onClick={() => setActiveView("diagnostics")}
              title="Diagnostics"
              aria-label="Diagnostics"
            >
              <TerminalSquare size={18} />
            </button>
          ) : null}
        </nav>
        <div className={`rail-status ${runtime.connectionState}`} title={runtime.connectionState}>
          <CircleDot size={14} />
        </div>
      </aside>

      {visibleActiveView === "home" ? (
        <aside className="thread-panel">
          <ContinuityPanel runtime={runtime} />
        </aside>
      ) : null}

      <main className="workspace">
        <TopBar
          activeView={visibleActiveView}
          preferences={preferences}
          runtime={runtime}
          setPreferences={setPreferences}
        />
        <WorkspaceView
          activeView={visibleActiveView}
          preferences={preferences}
          runtime={runtime}
          setPreferences={setPreferences}
        />
      </main>

      {visibleActiveView === "diagnostics" ? null : null}
    </div>
  );
}

function ContinuityPanel({ runtime }: { runtime: LizRuntime }) {
  const [search, setSearch] = useState("");
  const filteredThreads = runtime.state.threads.filter((thread) =>
    `${thread.title} ${thread.active_summary ?? ""} ${thread.workspace_ref ?? ""}`
      .toLowerCase()
      .includes(search.toLowerCase()),
  );
  const activeThreads = filteredThreads.filter((thread) =>
    ["active", "waiting_approval", "interrupted"].includes(thread.status),
  );
  const completedThreads = filteredThreads.filter((thread) =>
    ["completed", "failed", "archived"].includes(thread.status),
  );

  return (
    <>
      <header className="panel-header">
          <div>
            <p className="eyebrow">Continuity</p>
            <h1>liz</h1>
          </div>
          <button
            className="icon-button"
            type="button"
            title="Refresh continuity"
            aria-label="Refresh continuity"
            onClick={() => void runtime.refreshThreads()}
          >
            <MessageSquareText size={17} />
          </button>
        </header>

        <label className="search-field">
          <Search size={15} />
          <input
            placeholder="Search what we're carrying"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </label>

        <div className="thread-list">
          <ThreadGroup
            label="Still carrying"
            threads={activeThreads}
            runtime={runtime}
            empty="Nothing active yet. Send a message in Home to start a new line."
          />
          <ThreadGroup
            label="Completed or parked"
            threads={completedThreads}
            runtime={runtime}
            empty="No completed lines loaded."
          />
        </div>

        <section className="side-section">
          <div className="section-row">
          <span>Workspace</span>
            <strong>{runtime.activeThread ? workspaceLabel(runtime.activeThread) : "None"}</strong>
          </div>
          <div className="section-row">
            <span>Channel</span>
            <strong>Web owner</strong>
          </div>
          <div className="section-row">
            <span>Connection</span>
            <strong>{runtime.connectionState}</strong>
          </div>
        </section>
      </>
  );
}

function ThreadGroup({
  label,
  threads,
  runtime,
  empty,
}: {
  label: string;
  threads: Thread[];
  runtime: LizRuntime;
  empty: string;
}) {
  return (
    <section className="continuity-group">
      <p className="eyebrow">{label}</p>
      {threads.map((thread) => (
        <button
          key={thread.id}
          className={runtime.activeThread?.id === thread.id ? "thread-item active" : "thread-item"}
          onClick={() => void runtime.setActiveThread(thread.id)}
        >
          <span>
            <strong>{thread.title}</strong>
            <small>{thread.active_summary ?? thread.active_goal ?? "No summary yet"}</small>
            <em>
              <span className={`thread-status-label ${thread.status}`}>{thread.status}</span>
              {workspaceLabel(thread)}
            </em>
          </span>
        </button>
      ))}
      {threads.length === 0 ? <div className="empty-panel">{empty}</div> : null}
    </section>
  );
}

function TopBar({
  activeView,
  preferences,
  runtime,
  setPreferences,
}: {
  activeView: ViewId;
  preferences: Preferences;
  runtime: LizRuntime;
  setPreferences: React.Dispatch<React.SetStateAction<Preferences>>;
}) {
  return (
    <header className="top-bar">
      <div>
        <p className="eyebrow">{activeView}</p>
        <h2>{viewTitle(activeView)}</h2>
      </div>
      <div className="top-actions">
        <label className="server-pill">
          <PlugZap size={15} />
          <input
            value={preferences.serverUrl}
            onChange={(event) =>
              setPreferences((current) => ({ ...current, serverUrl: event.target.value }))
            }
            aria-label="Server URL"
          />
        </label>
        <button className="primary-button" type="button" onClick={runtime.connect}>
          {runtime.connectionState === "connected" ? "Reconnect" : "Connect"}
        </button>
      </div>
      {runtime.error ? <div className="top-error">{runtime.error}</div> : null}
    </header>
  );
}

function PeopleSurface({ runtime }: { runtime: LizRuntime }) {
  const aboutYou = runtime.state.aboutYou;
  const knowledge = runtime.state.knowledge;
  const people = runtime.state.people;
  const [identitySummaryDraft, setIdentitySummaryDraft] = useState<string | null>(null);
  const [profileDraft, setProfileDraft] = useState<Record<string, string>>({});
  const [correctionDrafts, setCorrectionDrafts] = useState<Record<string, string>>({});
  const [personDraft, setPersonDraft] = useState<PersonBoundary>(emptyPersonBoundary());
  const hasOnboardingProfile = !aboutYou || aboutYou.items.length === 0;
  const profileItems = hasOnboardingProfile ? defaultAboutYouItems() : (aboutYou?.items ?? []);
  const identitySummary =
    identitySummaryDraft ??
    aboutYou?.identity_summary ??
    "Liz can remember my preferences, keep work continuity, and ask before sharing anything sensitive.";

  const saveAboutYou = () => {
    const items = profileItems.map((item) => ({
      ...item,
      value: profileDraft[item.key] ?? item.value,
      confirmed: true,
    }));
    void runtime.updateAboutYou({
      identity_summary: identitySummary.trim() || null,
      items,
    });
    setIdentitySummaryDraft(null);
    setProfileDraft({});
  };

  const correctKnowledge = (item: KnowledgeItem) => {
    const correctedValue = correctionDrafts[item.fact_id]?.trim();
    if (!correctedValue) {
      return;
    }
    void runtime.correctKnowledge({
      fact_id: item.fact_id,
      corrected_value: correctedValue,
    });
    setCorrectionDrafts((current) => ({ ...current, [item.fact_id]: "" }));
  };

  const savePersonBoundary = () => {
    const personId = personDraft.person_id.trim();
    const displayName = personDraft.display_name.trim();
    if (!personId || !displayName) {
      return;
    }
    void runtime.upsertPersonBoundary({
      ...personDraft,
      person_id: personId,
      display_name: displayName,
      interaction_stance:
        personDraft.interaction_stance.trim() ||
        (personDraft.actor_kind === "external_agent" ? "agent_task_scoped" : "bounded_contact"),
      shared_topics: personDraft.shared_topics.map((topic) => topic.trim()).filter(Boolean),
      forbidden_topics: personDraft.forbidden_topics.map((topic) => topic.trim()).filter(Boolean),
      notes: personDraft.notes?.trim() || null,
    });
    setPersonDraft(emptyPersonBoundary());
  };

  return (
    <section className="settings-surface control-surface">
      <div className="settings-header">
        <div>
          <UserRound size={20} />
          <h2>People</h2>
        </div>
        <button
          className="secondary-button"
          type="button"
          onClick={() => {
            void runtime.loadOwnerSurfaces();
            void runtime.loadPeopleSurface();
          }}
        >
          Refresh
        </button>
      </div>
      <div className="settings-grid">
        <section className="settings-section">
          <p className="eyebrow">About You</p>
          {hasOnboardingProfile ? (
            <div className="empty-panel onboarding-panel">
              <strong>Start with a first profile</strong>
              <p>
                liz will use this as the starting point for your preferences, update style, and
                communication boundaries.
              </p>
            </div>
          ) : null}
          <textarea
            className="small-textarea"
            value={identitySummary}
            onChange={(event) => setIdentitySummaryDraft(event.target.value)}
            placeholder="How liz should summarize the owner profile"
          />
          <div className="editable-list">
            {profileItems.map((item) => (
              <article key={item.key}>
                <label>
                  <span>{item.label}</span>
                  <input
                    value={profileDraft[item.key] ?? item.value}
                    onChange={(event) =>
                      setProfileDraft((current) => ({ ...current, [item.key]: event.target.value }))
                    }
                  />
                </label>
                <small>{item.confirmed ? "confirmed" : "learned"}</small>
              </article>
            ))}
            {profileItems.length === 0 ? (
              <div className="empty-panel">No owner profile details are confirmed yet.</div>
            ) : null}
          </div>
          <button className="primary-button" type="button" onClick={saveAboutYou}>
            {hasOnboardingProfile ? "Create About You" : "Save About You"}
          </button>
        </section>
        <section className="settings-section provider-section">
          <p className="eyebrow">Boundaries</p>
          <div className="provider-form people-boundary-form">
            <select
              value={personDraft.actor_kind}
              aria-label="Actor kind"
              onChange={(event) =>
                setPersonDraft((current) => ({
                  ...current,
                  actor_kind: event.target.value as PersonBoundary["actor_kind"],
                }))
              }
            >
              <option value="human">Human</option>
              <option value="external_agent">External agent</option>
            </select>
            <input
              value={personDraft.person_id}
              onChange={(event) =>
                setPersonDraft((current) => ({ ...current, person_id: event.target.value }))
              }
              placeholder="Actor id"
              aria-label="Actor id"
            />
            <input
              value={personDraft.display_name}
              onChange={(event) =>
                setPersonDraft((current) => ({ ...current, display_name: event.target.value }))
              }
              placeholder="Display name"
              aria-label="Display name"
            />
            <select
              value={personDraft.trust_level}
              aria-label="Trust level"
              onChange={(event) =>
                setPersonDraft((current) => ({
                  ...current,
                  trust_level: event.target.value as PersonBoundary["trust_level"],
                }))
              }
            >
              <option value="trusted">Trusted</option>
              <option value="acquaintance">Acquaintance</option>
              <option value="stranger">Stranger</option>
            </select>
            <input
              value={personDraft.shared_topics.join(", ")}
              onChange={(event) =>
                setPersonDraft((current) => ({
                  ...current,
                  shared_topics: splitTopics(event.target.value),
                }))
              }
              placeholder="Shared topics"
              aria-label="Shared topics"
            />
            <input
              value={personDraft.forbidden_topics.join(", ")}
              onChange={(event) =>
                setPersonDraft((current) => ({
                  ...current,
                  forbidden_topics: splitTopics(event.target.value),
                }))
              }
              placeholder="Forbidden topics"
              aria-label="Forbidden topics"
            />
            <label>
              <input
                type="checkbox"
                checked={personDraft.share_active_state}
                onChange={(event) =>
                  setPersonDraft((current) => ({
                    ...current,
                    share_active_state: event.target.checked,
                  }))
                }
              />
              <span>Active state</span>
            </label>
            <label>
              <input
                type="checkbox"
                checked={personDraft.share_commitments}
                onChange={(event) =>
                  setPersonDraft((current) => ({
                    ...current,
                    share_commitments: event.target.checked,
                  }))
                }
              />
              <span>Commitments</span>
            </label>
            <button className="primary-button" type="button" onClick={savePersonBoundary}>
              Save boundary
            </button>
          </div>
          <PeopleBoundaryList
            title="Known humans"
            people={people?.humans ?? []}
            onDelete={runtime.deletePersonBoundary}
          />
          <PeopleBoundaryList
            title="External agents"
            people={people?.external_agents ?? []}
            onDelete={runtime.deletePersonBoundary}
          />
          {!people ? <div className="empty-panel">No people boundaries loaded.</div> : null}
        </section>
        <section className="settings-section provider-section">
          <p className="eyebrow">Knowledge / Decisions</p>
          <KnowledgeList
            items={knowledge?.items ?? []}
            correctionDrafts={correctionDrafts}
            setCorrectionDrafts={setCorrectionDrafts}
            onCorrect={correctKnowledge}
          />
        </section>
      </div>
    </section>
  );
}

function DevicesSurface({ runtime }: { runtime: LizRuntime }) {
  return (
    <section className="settings-surface control-surface">
      <div className="settings-header">
        <div>
          <HardDrive size={20} />
          <h2>Devices</h2>
        </div>
        <button className="secondary-button" type="button" onClick={() => void runtime.loadRuntimeState()}>
          Refresh
        </button>
      </div>
      <div className="channel-list">
        {runtime.state.nodes.map((node) => (
          <article className="channel-row" key={node.identity.node_id}>
            <div>
              <span>{node.identity.kind}</span>
              <strong>{node.identity.display_name}</strong>
              <p>
                {node.status.online ? "online" : "offline"}
                {node.status.os ? ` on ${node.status.os}` : ""}
                {node.status.hostname ? ` · ${node.status.hostname}` : ""}
              </p>
            </div>
            <small>
              {[
                node.capabilities.workspace_tools ? "workspace" : null,
                node.capabilities.shell_tools ? "shell" : null,
                node.capabilities.web_ui_host ? "web host" : null,
              ]
                .filter(Boolean)
                .join(" / ")}
            </small>
          </article>
        ))}
        {runtime.state.nodes.length === 0 ? <div className="empty-panel">No nodes loaded.</div> : null}
      </div>
    </section>
  );
}

function PeopleBoundaryList({
  title,
  people,
  onDelete,
}: {
  title: string;
  people: PersonBoundary[];
  onDelete: (personId: string) => Promise<void>;
}) {
  return (
    <div className="channel-list people-boundary-list">
      <p className="eyebrow">{title}</p>
      {people.map((person) => (
        <article className="channel-row" key={person.person_id}>
          <div>
            <span>{person.trust_level}</span>
            <strong>{person.display_name}</strong>
            <p>
              {[
                person.shared_topics.length
                  ? `shared: ${person.shared_topics.join(", ")}`
                  : "no shared topics",
                person.forbidden_topics.length
                  ? `forbidden: ${person.forbidden_topics.join(", ")}`
                  : null,
              ]
                .filter(Boolean)
                .join(" · ")}
            </p>
          </div>
          <small>
            {[
              person.share_active_state ? "active state" : null,
              person.share_commitments ? "commitments" : null,
              person.requires_owner_confirmation ? "confirm" : null,
            ]
              .filter(Boolean)
              .join(" / ") || "minimal"}
          </small>
          <button
            className="icon-button"
            type="button"
            aria-label={`Delete ${person.display_name}`}
            title="Delete boundary"
            onClick={() => void onDelete(person.person_id)}
          >
            <Trash2 size={16} />
          </button>
        </article>
      ))}
      {people.length === 0 ? <div className="empty-panel">No entries.</div> : null}
    </div>
  );
}

function WorkspacesSurface({ runtime }: { runtime: LizRuntime }) {
  const firstNode = runtime.state.nodes[0]?.identity.node_id ?? "local";
  const [nodeId, setNodeId] = useState(firstNode);
  const [rootPath, setRootPath] = useState("");
  const [label, setLabel] = useState("");
  const [permissions, setPermissions] = useState({ read: true, write: true, shell: true });
  const selectedNodeId = runtime.state.nodes.some((node) => node.identity.node_id === nodeId)
    ? nodeId
    : firstNode;

  const attachWorkspace = () => {
    const root = rootPath.trim();
    if (!root) {
      return;
    }
    void runtime.attachWorkspaceMount({
      node_id: selectedNodeId,
      root_path: root,
      label: label.trim() || null,
      permissions,
    });
    setRootPath("");
    setLabel("");
  };

  return (
    <section className="settings-surface control-surface">
      <div className="settings-header">
        <div>
          <FolderKanban size={20} />
          <h2>Workspaces</h2>
        </div>
        <button className="secondary-button" type="button" onClick={() => void runtime.loadRuntimeState()}>
          Refresh
        </button>
      </div>
      <div className="provider-form workspace-mount-form">
        <select value={selectedNodeId} onChange={(event) => setNodeId(event.target.value)} aria-label="Workspace node">
          {runtime.state.nodes.length === 0 ? <option value="local">Local device</option> : null}
          {runtime.state.nodes.map((node) => (
            <option key={node.identity.node_id} value={node.identity.node_id}>
              {node.identity.display_name}
            </option>
          ))}
        </select>
        <input
          value={rootPath}
          onChange={(event) => setRootPath(event.target.value)}
          placeholder="Workspace root path"
          aria-label="Workspace root path"
        />
        <input
          value={label}
          onChange={(event) => setLabel(event.target.value)}
          placeholder="Label"
          aria-label="Workspace label"
        />
        <div className="permission-row" aria-label="Workspace permissions">
          {(["read", "write", "shell"] as const).map((permission) => (
            <label key={permission}>
              <input
                type="checkbox"
                checked={permissions[permission]}
                onChange={(event) =>
                  setPermissions((current) => ({ ...current, [permission]: event.target.checked }))
                }
              />
              <span>{permission}</span>
            </label>
          ))}
        </div>
        <button className="primary-button" type="button" onClick={attachWorkspace}>
          Attach
        </button>
      </div>
      <div className="channel-list">
        {runtime.state.workspaceMounts.map((mount) => (
          <article className="channel-row" key={mount.workspace_id}>
            <div>
              <span>{mount.node_id}</span>
              <strong>{mount.label}</strong>
              <p>{mount.root_path}</p>
            </div>
            <small>
              {[
                mount.permissions.read ? "read" : null,
                mount.permissions.write ? "write" : null,
                mount.permissions.shell ? "shell" : null,
              ]
                .filter(Boolean)
                .join(" / ")}
            </small>
            <button
              className="icon-button"
              type="button"
              aria-label={`Detach ${mount.label}`}
              title="Detach workspace"
              onClick={() => void runtime.detachWorkspaceMount(mount.workspace_id)}
            >
              <Trash2 size={16} />
            </button>
          </article>
        ))}
        {runtime.state.workspaceMounts.length === 0 ? (
          <div className="empty-panel">No workspace mounts are attached yet.</div>
        ) : null}
      </div>
    </section>
  );
}

function emptyPersonBoundary(): PersonBoundary {
  return {
    person_id: "",
    display_name: "",
    actor_kind: "human",
    trust_level: "trusted",
    shared_topics: [],
    forbidden_topics: [],
    share_active_state: false,
    share_commitments: false,
    interaction_stance: "bounded_contact",
    notes: null,
    requires_owner_confirmation: true,
  };
}

function defaultAboutYouItems() {
  return [
    {
      key: "preferred_name",
      label: "How liz should address you",
      value: "Owner",
      confirmed: false,
      source_fact_id: null,
    },
    {
      key: "language",
      label: "Language preference",
      value: "Chinese",
      confirmed: false,
      source_fact_id: null,
    },
    {
      key: "update_style",
      label: "Update style",
      value: "Direct and concise",
      confirmed: false,
      source_fact_id: null,
    },
    {
      key: "autonomy_style",
      label: "Autonomy",
      value: "Ask before major changes",
      confirmed: false,
      source_fact_id: null,
    },
    {
      key: "work_style",
      label: "Work style",
      value: "Keep context narrow and continue from the current line",
      confirmed: false,
      source_fact_id: null,
    },
  ];
}

function splitTopics(value: string): string[] {
  return value.split(",").map((topic) => topic.trim()).filter(Boolean);
}

function DiagnosticsSurface({ runtime }: { runtime: LizRuntime }) {
  return (
    <section className="diagnostics-shell">
      <div className="diagnostics-main">
        <MemorySurface runtime={runtime} />
        <ApprovalsSurface runtime={runtime} />
      </div>
      <aside className="diagnostics-inspector">
        <Inspector runtime={runtime} />
      </aside>
    </section>
  );
}

function KnowledgeList({
  items,
  correctionDrafts,
  setCorrectionDrafts,
  onCorrect,
}: {
  items: KnowledgeItem[];
  correctionDrafts?: Record<string, string>;
  setCorrectionDrafts?: React.Dispatch<React.SetStateAction<Record<string, string>>>;
  onCorrect?: (item: KnowledgeItem) => void;
}) {
  if (items.length === 0) {
    return <div className="empty-panel">No decisions or knowledge items loaded.</div>;
  }

  return (
    <div className="topic-list">
      {items.map((item) => (
        <article key={item.fact_id}>
          <strong>{item.subject}</strong>
          <p>{item.summary}</p>
          <small>{item.stale ? "stale" : item.kind}</small>
          {setCorrectionDrafts && onCorrect ? (
            <div className="correction-row">
              <input
                value={correctionDrafts?.[item.fact_id] ?? ""}
                onChange={(event) =>
                  setCorrectionDrafts((current) => ({
                    ...current,
                    [item.fact_id]: event.target.value,
                  }))
                }
                placeholder="Correct this memory"
              />
              <button
                className="secondary-button"
                type="button"
                onClick={() => onCorrect(item)}
              >
                Correct
              </button>
            </div>
          ) : null}
        </article>
      ))}
    </div>
  );
}

function WorkspaceView({
  activeView,
  preferences,
  runtime,
  setPreferences,
}: {
  activeView: ViewId;
  preferences: Preferences;
  runtime: LizRuntime;
  setPreferences: React.Dispatch<React.SetStateAction<Preferences>>;
}) {
  if (activeView === "home") {
    return <ChatSurface runtime={runtime} />;
  }

  if (activeView === "settings") {
    return (
      <SettingsSurface
        preferences={preferences}
        runtime={runtime}
        setPreferences={setPreferences}
      />
    );
  }

  if (activeView === "people") {
    return <PeopleSurface runtime={runtime} />;
  }

  if (activeView === "devices") {
    return <DevicesSurface runtime={runtime} />;
  }

  if (activeView === "workspaces") {
    return <WorkspacesSurface runtime={runtime} />;
  }

  if (activeView === "channels") {
    return <ChannelsSurface runtime={runtime} />;
  }

  if (activeView === "diagnostics") {
    return <DiagnosticsSurface runtime={runtime} />;
  }

  return (
    <section className="placeholder-surface">
      <Bot size={22} />
      <h2>Not loaded</h2>
    </section>
  );
}

function MemorySurface({ runtime }: { runtime: LizRuntime }) {
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<"keyword" | "semantic">("keyword");
  const memory = runtime.activeMemory;
  const search = runtime.state.memorySearch;
  const evidence = runtime.state.selectedEvidence;

  return (
    <section className="memory-surface">
      <header>
        <div>
          <Brain size={20} />
          <h2>Memory</h2>
        </div>
        <div className="memory-actions">
          <button className="secondary-button" type="button" onClick={() => void runtime.readMemoryWakeup()}>
            Wakeup
          </button>
          <button className="secondary-button" type="button" onClick={() => void runtime.compileMemory()}>
            Compile now
          </button>
          <button className="secondary-button" type="button" onClick={() => void runtime.listMemoryTopics()}>
            Topics
          </button>
        </div>
      </header>

      <div className="memory-grid">
        <section>
          <p className="eyebrow">Liz remembers</p>
          <h3>{memory.wakeup?.identity_summary ?? "Wakeup not loaded"}</h3>
          <ListBlock items={memory.wakeup?.relevant_facts ?? []} empty="No relevant facts loaded." />
        </section>
        <section>
          <p className="eyebrow">Active state</p>
          <h3>{memory.wakeup?.active_state ?? "No active state loaded"}</h3>
          <ListBlock items={memory.wakeup?.open_commitments ?? []} empty="No open commitments loaded." />
        </section>
        <section>
          <p className="eyebrow">Compilation</p>
          <h3>{memory.compilation?.delta_summary ?? "No compilation summary"}</h3>
          <ListBlock
            items={memory.compilation?.recent_topics ?? []}
            empty="Compile memory to update topic changes."
          />
        </section>
        <section>
          <p className="eyebrow">Topics</p>
          <div className="topic-list">
            {runtime.state.memoryTopics.map((topic) => (
              <article key={topic.name}>
                <strong>{topic.name}</strong>
                <p>{topic.summary}</p>
                <small>{topic.status}</small>
              </article>
            ))}
            {runtime.state.memoryTopics.length === 0 ? (
              <div className="empty-panel">No topics loaded.</div>
            ) : null}
          </div>
        </section>
      </div>

      <form className="memory-search" onSubmit={(event) => event.preventDefault()}>
        <label className="search-field">
          <Search size={15} />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search memory"
          />
        </label>
        <select value={mode} onChange={(event) => setMode(event.target.value as "keyword" | "semantic")}>
          <option value="keyword">Keyword</option>
          <option value="semantic">Semantic</option>
        </select>
          <button
            className="primary-button"
            type="button"
            disabled={!query.trim()}
            onClick={() => void runtime.searchMemory(query, mode)}
          >
          Search
        </button>
      </form>

      <div className="memory-results">
        {search?.hits.map((hit) => (
          <button
            key={`${hit.kind}:${hit.title}:${hit.score}`}
            type="button"
            onClick={() => {
              if (!hit.thread_id) {
                return;
              }
              void runtime.openMemoryEvidence({
                thread_id: hit.thread_id,
                turn_id: hit.turn_id,
                artifact_id: hit.artifact_id,
                fact_id: hit.fact_id,
              });
            }}
          >
            <span>{hit.kind}</span>
            <strong>{hit.title}</strong>
            <p>{hit.summary}</p>
          </button>
        ))}
      </div>

      {evidence ? (
        <section className="evidence-view">
          <p className="eyebrow">Evidence</p>
          <h3>{evidence.thread_title ?? evidence.citation.note}</h3>
          <p>{evidence.fact_value ?? evidence.turn_summary ?? evidence.artifact?.summary}</p>
          {evidence.artifact_body ? <pre className="inspector-code">{evidence.artifact_body}</pre> : null}
        </section>
      ) : null}
    </section>
  );
}

function ListBlock({ items, empty }: { items: string[]; empty: string }) {
  if (items.length === 0) {
    return <p className="muted">{empty}</p>;
  }

  return (
    <ul className="memory-list">
      {items.map((item) => (
        <li key={item}>{item}</li>
      ))}
    </ul>
  );
}

function ApprovalsSurface({ runtime }: { runtime: LizRuntime }) {
  const approvals = runtime.allApprovals;

  return (
    <section className="approval-surface">
      <header>
        <ShieldCheck size={20} />
        <h2>Approvals</h2>
      </header>
      <div className="approval-list">
        {approvals.map((approval) => (
          <article key={approval.id} className={`approval-row ${approval.risk_level}`}>
            <div>
              <span>{approval.risk_level}</span>
              <h3>{approval.action_type}</h3>
              <p>{approval.reason}</p>
              {approval.sandbox_context ? <small>{approval.sandbox_context}</small> : null}
            </div>
            <div className="approval-actions">
              <strong>{approval.status}</strong>
              {approval.status === "pending" ? (
                <>
                  <button
                    className="secondary-button"
                    type="button"
                    onClick={() =>
                      void runtime.respondToApproval({
                        approval_id: approval.id,
                        decision: "deny",
                      })
                    }
                  >
                    Deny
                  </button>
                  <button
                    className="primary-button"
                    type="button"
                    onClick={() =>
                      void runtime.respondToApproval({
                        approval_id: approval.id,
                        decision: "approve_once",
                      })
                    }
                  >
                    Approve once
                  </button>
                </>
              ) : null}
            </div>
          </article>
        ))}
        {approvals.length === 0 ? <div className="empty-panel">No approvals loaded.</div> : null}
      </div>
    </section>
  );
}

function ChatSurface({ runtime }: { runtime: LizRuntime }) {
  const [message, setMessage] = useState("");
  const [inputKind, setInputKind] =
    useState<"user_message" | "steering_note" | "resume_command">("user_message");
  const submit = () => {
    void runtime.startTurn(message, inputKind);
    setMessage("");
  };
  const activeTurnId = runtime.activeRuntime.activeTurnId;
  const pendingApprovals = runtime.activeApprovals.filter((approval) => approval.status === "pending");
  const carrying = runtime.state.carrying?.active ?? [];

  return (
    <section className="chat-surface">
      <div className="transcript">
        {runtime.activeThread ? (
          <>
            {runtime.activeResumePanel ? (
              <ResumeBanner panel={runtime.activeResumePanel} />
            ) : null}
            {runtime.activeTranscript.length > 0 ? (
              runtime.activeTranscript.map((entry) => <TranscriptRow key={entry.id} entry={entry} />)
            ) : (
              <div className="empty-panel">Say what you want to pick up or start.</div>
            )}
          </>
        ) : (
          <HomeOverview runtime={runtime} carryingCount={carrying.length} />
        )}
        {pendingApprovals.map((approval) => (
          <article className={`approval-row inline-approval ${approval.risk_level}`} key={approval.id}>
            <div>
              <span>Needs confirmation</span>
              <h3>{approval.action_type}</h3>
              <p>{approval.reason}</p>
              {approval.sandbox_context ? <small>{approval.sandbox_context}</small> : null}
            </div>
            <div className="approval-actions">
              <button
                className="secondary-button"
                type="button"
                onClick={() =>
                  void runtime.respondToApproval({
                    approval_id: approval.id,
                    decision: "deny",
                  })
                }
              >
                Deny
              </button>
              <button
                className="primary-button"
                type="button"
                onClick={() =>
                  void runtime.respondToApproval({
                    approval_id: approval.id,
                    decision: "approve_once",
                  })
                }
              >
                Approve once
              </button>
            </div>
          </article>
        ))}
        {runtime.activeToolCalls.length > 0 ? (
          <details className="tool-timeline" aria-label="Tool timeline">
            <summary>Working details</summary>
            {runtime.activeToolCalls.map((toolCall) => (
              <button
                key={toolCall.callId}
                className={`tool-line ${toolCall.status} ${toolCall.riskHint ?? ""}`}
                type="button"
                onClick={() => runtime.selectToolCall(toolCall.callId)}
              >
                <TerminalSquare size={16} />
                <div>
                  <strong>{toolCall.toolName}</strong>
                  <p>{toolCall.summary || toolCall.argumentsSummary || "Tool call"}</p>
                </div>
                <span>{toolCall.status}</span>
              </button>
            ))}
          </details>
        ) : null}
      </div>
      <form className="composer">
        <div className="composer-toolbar">
          <label>
            <Command size={14} />
            <select
              value={inputKind}
              onChange={(event) =>
                setInputKind(event.target.value as "user_message" | "steering_note" | "resume_command")
              }
              aria-label="Message mode"
            >
              <option value="user_message">Message</option>
              <option value="steering_note">Steering</option>
              <option value="resume_command">Resume</option>
            </select>
          </label>
          <span>{runtime.activeThread ? workspaceLabel(runtime.activeThread) : "new line starts when you send"}</span>
        </div>
        <textarea
          placeholder="Message Liz"
          rows={3}
          value={message}
          onChange={(event) => setMessage(event.target.value)}
          onKeyDown={(event) => {
            if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
              submit();
            }
          }}
        />
        <div className="composer-actions">
          <span>Ctrl Enter to send</span>
          {activeTurnId ? (
            <button
              className="secondary-button danger-button"
              type="button"
              onClick={() => void runtime.cancelTurn()}
            >
              <Square size={14} />
              Stop
            </button>
          ) : null}
          <button
            className="primary-button"
            type="button"
            disabled={!message.trim()}
            onClick={submit}
          >
            <Send size={14} />
            Send
          </button>
        </div>
      </form>
    </section>
  );
}

function HomeOverview({ runtime, carryingCount }: { runtime: LizRuntime; carryingCount: number }) {
  const carrying = runtime.state.carrying;
  const activeItems = carrying?.active.slice(0, 4) ?? [];
  const completedItems = carrying?.completed.slice(0, 3) ?? [];
  const commitments = activeItems.flatMap((item) => item.pending_commitments).slice(0, 4);
  const knowledge = runtime.state.knowledge?.items.slice(0, 3) ?? [];

  return (
    <section className="home-overview">
      <p className="eyebrow">Home</p>
      <h2>{runtime.state.aboutYou?.identity_summary ?? "liz is here."}</h2>
      <div className="metric-grid">
        <div>
          <span>Active lines</span>
          <strong>{carryingCount}</strong>
        </div>
        <div>
          <span>Pending confirmations</span>
          <strong>{runtime.allApprovals.filter((approval) => approval.status === "pending").length}</strong>
        </div>
      </div>
      <div className="home-continuity-grid">
        <section>
          <p className="eyebrow">Still carrying</p>
          {activeItems.map((item) => (
            <article key={item.thread_id}>
              <strong>{item.title}</strong>
              <p>{item.summary ?? item.suggested_next_step ?? "No summary yet"}</p>
            </article>
          ))}
          {activeItems.length === 0 ? <div className="empty-panel">Nothing active is loaded yet.</div> : null}
        </section>
        <section>
          <p className="eyebrow">Commitments</p>
          {commitments.map((commitment) => (
            <article key={commitment}>
              <p>{commitment}</p>
            </article>
          ))}
          {commitments.length === 0 ? <div className="empty-panel">No pending commitments loaded.</div> : null}
        </section>
        <section>
          <p className="eyebrow">Key memory</p>
          {knowledge.map((item) => (
            <article key={item.fact_id}>
              <strong>{item.subject}</strong>
              <p>{item.summary}</p>
            </article>
          ))}
          {knowledge.length === 0 ? <div className="empty-panel">No decisions are loaded yet.</div> : null}
        </section>
        <section>
          <p className="eyebrow">Recently completed</p>
          {completedItems.map((item) => (
            <article key={item.thread_id}>
              <strong>{item.title}</strong>
              <p>{item.summary ?? "Completed line"}</p>
            </article>
          ))}
          {completedItems.length === 0 ? <div className="empty-panel">No completed lines loaded.</div> : null}
        </section>
      </div>
    </section>
  );
}

function ResumeBanner({
  panel,
}: {
  panel: NonNullable<LizRuntime["activeResumePanel"]>;
}) {
  return (
    <section className="resume-banner">
      <span>Resume</span>
      <div>
        <strong>{panel.headline}</strong>
        {panel.activeSummary ? <p>{panel.activeSummary}</p> : null}
        {panel.lastInterruption ? <small>{panel.lastInterruption}</small> : null}
      </div>
      {panel.pendingCommitments.length > 0 ? (
        <ul>
          {panel.pendingCommitments.slice(0, 3).map((commitment) => (
            <li key={commitment}>{commitment}</li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}

function TranscriptRow({ entry }: { entry: TranscriptEntry }) {
  if (entry.kind === "system") {
    return (
      <article className={`message system ${entry.tone}`}>
        <span>System</span>
        <p>{entry.content}</p>
      </article>
    );
  }

  return (
    <article className={`message ${entry.kind} ${entry.status}`}>
      <span>{entry.kind === "user" ? "User" : "Liz"}</span>
      <p>{entry.content || (entry.kind === "assistant" ? "Thinking..." : "")}</p>
    </article>
  );
}

function ChannelsSurface({ runtime }: { runtime: LizRuntime }) {
  const telegramProfiles = runtime.state.providerProfiles.filter((profile) =>
    profile.provider_id.toLowerCase().includes("telegram"),
  );

  return (
    <section className="settings-surface">
      <div className="settings-header">
        <div>
          <PlugZap size={20} />
          <h2>Channels</h2>
        </div>
        <button className="secondary-button" type="button" onClick={() => void runtime.loadRuntimeState()}>
          Refresh
        </button>
      </div>
      <div className="channel-list">
        <article className="channel-row">
          <div>
            <span>Telegram</span>
            <strong>{telegramProfiles.length > 0 ? "configured" : "not configured"}</strong>
            <p>Adapter status, last error, and activity timestamps are not exposed by the server yet.</p>
          </div>
          <small>Token hidden</small>
        </article>
        {["Discord", "Email", "Unknown"].map((channel) => (
          <article key={channel} className="channel-row disabled">
            <div>
              <span>{channel}</span>
              <strong>not exposed by server yet</strong>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

function SettingsSurface({
  preferences,
  runtime,
  setPreferences,
}: {
  preferences: Preferences;
  runtime: LizRuntime;
  setPreferences: React.Dispatch<React.SetStateAction<Preferences>>;
}) {
  const [providerId, setProviderId] = useState("");
  const [profileId, setProfileId] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [secret, setSecret] = useState("");

  const saveProvider = () => {
    if (!providerId.trim() || !profileId.trim() || !secret.trim()) {
      return;
    }
    void runtime.upsertProviderProfile({
      provider_id: providerId.trim(),
      profile_id: profileId.trim(),
      display_name: displayName.trim() || null,
      credential: { kind: "api_key", api_key: secret },
    });
    setSecret("");
  };

  return (
    <section className="settings-surface">
      <div className="settings-header">
        <div>
          <Settings size={20} />
          <h2>Settings</h2>
        </div>
        <button className="secondary-button" type="button" onClick={() => void runtime.loadRuntimeState()}>
          Load runtime
        </button>
      </div>
      <div className="settings-grid">
        <section className="settings-section">
          <p className="eyebrow">Connection</p>
          <div className="setting-row">
            <span>Server URL</span>
            <input
              value={preferences.serverUrl}
              onChange={(event) =>
                setPreferences((current) => ({ ...current, serverUrl: event.target.value }))
              }
            />
          </div>
          <div className="setting-row">
            <span>Connection state</span>
            <strong>{runtime.connectionState}</strong>
          </div>
          <div className="setting-row">
            <span>Browser instance</span>
            <strong>{preferences.browserInstanceId}</strong>
          </div>
        </section>

        <section className="settings-section">
          <p className="eyebrow">Developer</p>
          <label className="setting-row checkbox-setting">
            <span>Diagnostics</span>
            <input
              type="checkbox"
              checked={preferences.developerMode}
              onChange={(event) =>
                setPreferences((current) => ({
                  ...current,
                  developerMode: event.target.checked,
                }))
              }
              aria-label="Enable Diagnostics"
            />
          </label>
        </section>

        <section className="settings-section">
          <p className="eyebrow">Appearance</p>
          <div className="setting-row">
            <span>Theme</span>
            <select
              value={preferences.theme}
              onChange={(event) =>
                setPreferences((current) => ({
                  ...current,
                  theme: event.target.value as Preferences["theme"],
                }))
              }
            >
              <option value="system">System</option>
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </select>
          </div>
          <div className="setting-row">
            <span>Density</span>
            <select
              value={preferences.density}
              onChange={(event) =>
                setPreferences((current) => ({
                  ...current,
                  density: event.target.value as Preferences["density"],
                }))
              }
            >
              <option value="comfortable">Comfortable</option>
              <option value="compact">Compact</option>
            </select>
          </div>
          <div className="setting-row">
            <span>Markdown</span>
            <select
              value={preferences.markdown}
              onChange={(event) =>
                setPreferences((current) => ({
                  ...current,
                  markdown: event.target.value as Preferences["markdown"],
                }))
              }
            >
              <option value="rendered">Rendered</option>
              <option value="plain">Plain</option>
            </select>
          </div>
          <div className="setting-row">
            <span>Tool cards</span>
            <select
              value={preferences.toolVerbosity}
              onChange={(event) =>
                setPreferences((current) => ({
                  ...current,
                  toolVerbosity: event.target.value as Preferences["toolVerbosity"],
                }))
              }
            >
              <option value="brief">Brief</option>
              <option value="detailed">Detailed</option>
            </select>
          </div>
        </section>

        <section className="settings-section">
          <p className="eyebrow">Runtime</p>
          <div className="setting-row">
            <span>Approval policy</span>
            <select
              value={runtime.state.runtimeConfig?.approval_policy ?? "on-request"}
              onChange={(event) =>
                void runtime.updateRuntimeConfig({
                  sandbox: null,
                  approval_policy: event.target.value as "on-request" | "danger-full-access",
                })
              }
            >
              <option value="on-request">On request</option>
              <option value="danger-full-access">Danger full access</option>
            </select>
          </div>
          <div className="setting-row">
            <span>Sandbox mode</span>
            <strong>{runtime.state.runtimeConfig?.sandbox.mode ?? "not loaded"}</strong>
          </div>
          <div className="setting-row">
            <span>Network</span>
            <strong>{runtime.state.runtimeConfig?.sandbox.network ?? "not loaded"}</strong>
          </div>
          <div className="setting-row">
            <span>Working directory</span>
            <strong>{runtime.state.runtimeConfig?.sandbox.working_directory ?? "not set"}</strong>
          </div>
        </section>

        <section className="settings-section">
          <p className="eyebrow">Model</p>
          <div className="setting-row">
            <span>Provider</span>
            <strong>{runtime.state.modelStatus?.provider_id ?? "not loaded"}</strong>
          </div>
          <div className="setting-row">
            <span>Model</span>
            <strong>{runtime.state.modelStatus?.model_id ?? "not selected"}</strong>
          </div>
          <div className="setting-row">
            <span>Credential</span>
            <strong>{runtime.state.modelStatus?.credential_configured ? "configured" : "missing"}</strong>
          </div>
          <div className="setting-row">
            <span>Ready</span>
            <strong>{runtime.state.modelStatus?.ready ? "yes" : "no"}</strong>
          </div>
        </section>

        <section className="settings-section provider-section">
          <p className="eyebrow">Provider auth</p>
          <div className="provider-form">
            <input value={providerId} onChange={(event) => setProviderId(event.target.value)} placeholder="Provider id" />
            <input value={profileId} onChange={(event) => setProfileId(event.target.value)} placeholder="Profile id" />
            <input value={displayName} onChange={(event) => setDisplayName(event.target.value)} placeholder="Display name" />
            <input value={secret} onChange={(event) => setSecret(event.target.value)} placeholder="API key or token" type="password" />
            <button className="primary-button" type="button" onClick={saveProvider}>
              Save profile
            </button>
          </div>
          <div className="provider-list">
            {runtime.state.providerProfiles.map((profile) => (
              <article key={profile.profile_id}>
                <div>
                  <strong>{profile.display_name ?? profile.profile_id}</strong>
                  <small>{profile.provider_id}</small>
                </div>
                <button
                  className="secondary-button"
                  type="button"
                  onClick={() => void runtime.deleteProviderProfile(profile.profile_id)}
                >
                  Delete
                </button>
              </article>
            ))}
            {runtime.state.providerProfiles.length === 0 ? (
              <div className="empty-panel">No provider profiles loaded.</div>
            ) : null}
          </div>
        </section>
      </div>
    </section>
  );
}

function Inspector({ runtime }: { runtime: LizRuntime }) {
  const [forkReason, setForkReason] = useState("");
  const selectedTool = runtime.selectedToolCall;

  return (
    <>
      <header className="panel-header compact">
        <div>
          <p className="eyebrow">Inspector</p>
          <h2>Selection</h2>
        </div>
        <button
          className="icon-button"
          type="button"
          title="Fork thread"
          aria-label="Fork thread"
          disabled={!runtime.activeThread}
          onClick={() => {
            if (!runtime.activeThread) {
              return;
            }
            void runtime.forkThread({
              thread_id: runtime.activeThread.id,
              title: `${runtime.activeThread.title} fork`,
              fork_reason: forkReason.trim() || null,
            });
          }}
        >
          <GitFork size={16} />
        </button>
      </header>
      <div className="inspector-body">
        <section>
          {selectedTool ? (
            <>
              <p className="eyebrow">Tool detail</p>
              <h3>{selectedTool.toolName}</h3>
              <p className="muted">{selectedTool.summary}</p>
              {selectedTool.argumentsSummary ? (
                <pre className="inspector-code">{selectedTool.argumentsSummary}</pre>
              ) : null}
              {selectedTool.output.length > 0 ? (
                <pre className="inspector-code">
                  {selectedTool.output
                    .map((chunk) => `[${chunk.stream}] ${chunk.chunk}`)
                    .join("")}
                </pre>
              ) : null}
              {selectedTool.artifactIds.length > 0 ? (
                <div className="artifact-list">
                  {selectedTool.artifactIds.map((artifactId) => (
                    <span key={artifactId}>{artifactId}</span>
                  ))}
                </div>
              ) : null}
            </>
          ) : (
            <>
              <p className="eyebrow">Thread</p>
              <h3>{runtime.activeThread?.title ?? "No thread selected"}</h3>
              <p className="muted">
                {runtime.activeThread?.active_summary ??
                  "Tool output, approvals, artifacts, diffs, and memory evidence appear here when selected."}
              </p>
            </>
          )}
        </section>
        <section>
          <p className="eyebrow">Fork reason</p>
          <textarea
            className="small-textarea"
            value={forkReason}
            onChange={(event) => setForkReason(event.target.value)}
            placeholder="Reason for branching this thread"
          />
        </section>
        <section className="metric-grid">
          <div>
            <span>Turn</span>
            <strong>{runtime.activeRuntime.activeTurnId ? "Live" : "Idle"}</strong>
          </div>
          <div>
            <span>Pending approvals</span>
            <strong>
              {runtime.activeApprovals.filter((approval) => approval.status === "pending").length}
            </strong>
          </div>
        </section>
      </div>
    </>
  );
}

function viewTitle(view: ViewId) {
  switch (view) {
    case "home":
      return "Home";
    case "people":
      return "People";
    case "channels":
      return "Channels";
    case "devices":
      return "Devices";
    case "workspaces":
      return "Workspaces";
    case "settings":
      return "Settings";
    case "diagnostics":
      return "Diagnostics";
  }
}

function workspaceLabel(thread: Thread) {
  return thread.workspace_ref?.trim() ? thread.workspace_ref : "conversation-only";
}
