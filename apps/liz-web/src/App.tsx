import {
  Bot,
  Brain,
  CheckSquare,
  CircleDot,
  GitFork,
  MessageSquareText,
  PlugZap,
  Search,
  Settings,
  ShieldCheck,
  Sparkles,
  TerminalSquare,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useLizRuntime, type LizRuntime } from "./hooks/useLizRuntime";
import { loadPreferences, savePreferences, type Preferences } from "./preferences";
import type { Thread } from "./protocol/types";
import type { TranscriptEntry } from "./state/workbench";

type ViewId = "chat" | "memory" | "approvals" | "channels" | "settings";

const views: Array<{ id: ViewId; label: string; icon: React.ComponentType<{ size?: number }> }> = [
  { id: "chat", label: "Chat", icon: MessageSquareText },
  { id: "memory", label: "Memory", icon: Brain },
  { id: "approvals", label: "Approvals", icon: CheckSquare },
  { id: "channels", label: "Channels", icon: PlugZap },
  { id: "settings", label: "Settings", icon: Settings },
];

export function App() {
  const [activeView, setActiveView] = useState<ViewId>("chat");
  const [preferences, setPreferences] = useState<Preferences>(() => loadPreferences());
  const runtime = useLizRuntime(preferences);

  useEffect(() => {
    savePreferences(preferences);
  }, [preferences]);

  const shellClassName = useMemo(
    () => `console-shell density-${preferences.density} theme-${preferences.theme}`,
    [preferences.density, preferences.theme],
  );

  return (
    <div className={shellClassName}>
      <aside className="rail" aria-label="Primary navigation">
        <div className="mark" aria-label="Liz">
          <Sparkles size={18} />
        </div>
        <nav>
          {views.map((view) => {
            const Icon = view.icon;
            return (
              <button
                key={view.id}
                className={activeView === view.id ? "rail-button active" : "rail-button"}
                onClick={() => setActiveView(view.id)}
                title={view.label}
                aria-label={view.label}
              >
                <Icon size={18} />
              </button>
            );
          })}
        </nav>
        <div className={`rail-status ${runtime.connectionState}`} title={runtime.connectionState}>
          <CircleDot size={14} />
        </div>
      </aside>

      <aside className="thread-panel">
        <ThreadPanel runtime={runtime} />
      </aside>

      <main className="workspace">
        <TopBar
          activeView={activeView}
          preferences={preferences}
          runtime={runtime}
          setPreferences={setPreferences}
        />
        <WorkspaceView
          activeView={activeView}
          preferences={preferences}
          runtime={runtime}
          setPreferences={setPreferences}
        />
      </main>

      <aside className="inspector">
        <Inspector runtime={runtime} />
      </aside>
    </div>
  );
}

function ThreadPanel({ runtime }: { runtime: LizRuntime }) {
  const [search, setSearch] = useState("");
  const [newTitle, setNewTitle] = useState("");
  const [workspaceRef, setWorkspaceRef] = useState("");
  const filteredThreads = runtime.state.threads.filter((thread) =>
    `${thread.title} ${thread.active_summary ?? ""} ${thread.workspace_ref ?? ""}`
      .toLowerCase()
      .includes(search.toLowerCase()),
  );

  const createThread = () => {
    void runtime.startThread({
      title: newTitle.trim() || null,
      initial_goal: newTitle.trim() || null,
      workspace_ref: workspaceRef.trim() || null,
    });
    setNewTitle("");
  };

  return (
    <>
      <header className="panel-header">
          <div>
            <p className="eyebrow">Threads</p>
            <h1>Liz Console</h1>
          </div>
          <button
            className="icon-button"
            type="button"
            title="Refresh threads"
            aria-label="Refresh threads"
            onClick={() => void runtime.refreshThreads()}
          >
            <MessageSquareText size={17} />
          </button>
        </header>

        <label className="search-field">
          <Search size={15} />
          <input
            placeholder="Search threads"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </label>

        <form className="new-thread-form" onSubmit={(event) => event.preventDefault()}>
          <input
            value={newTitle}
            onChange={(event) => setNewTitle(event.target.value)}
            placeholder="New thread goal"
          />
          <input
            value={workspaceRef}
            onChange={(event) => setWorkspaceRef(event.target.value)}
            placeholder="Workspace path optional"
          />
          <button className="secondary-button" type="button" onClick={createThread}>
            New
          </button>
        </form>

        <div className="thread-list">
          {filteredThreads.map((thread) => (
            <button
              key={thread.id}
              className={runtime.activeThread?.id === thread.id ? "thread-item active" : "thread-item"}
              onClick={() => void runtime.setActiveThread(thread.id)}
            >
              <span className={`status-dot ${thread.status}`} />
              <span>
                <strong>{thread.title}</strong>
                <small>{thread.active_summary ?? thread.active_goal ?? "No summary yet"}</small>
                <em>{workspaceLabel(thread)}</em>
              </span>
            </button>
          ))}
          {filteredThreads.length === 0 ? (
            <div className="empty-panel">No threads loaded.</div>
          ) : null}
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
  if (activeView === "chat") {
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

  if (activeView === "approvals") {
    return <ApprovalsSurface runtime={runtime} />;
  }

  if (activeView === "memory") {
    return <MemorySurface runtime={runtime} />;
  }

  if (activeView === "channels") {
    return <ChannelsSurface runtime={runtime} />;
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
  const submit = () => {
    void runtime.startTurn(message);
    setMessage("");
  };

  return (
    <section className="chat-surface">
      <div className="transcript">
        {runtime.activeThread ? (
          runtime.activeTranscript.length > 0 ? (
            runtime.activeTranscript.map((entry) => <TranscriptRow key={entry.id} entry={entry} />)
          ) : (
            <div className="empty-panel">Start a turn to build this transcript.</div>
          )
        ) : (
          <div className="empty-panel">Connect to the app server and create or select a thread.</div>
        )}
        {runtime.activeToolCalls.length > 0 ? (
          <div className="tool-timeline" aria-label="Tool timeline">
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
          </div>
        ) : null}
      </div>
      <form className="composer">
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
        <div>
          <span>{runtime.activeThread ? workspaceLabel(runtime.activeThread) : "no thread selected"}</span>
          <button
            className="secondary-button"
            type="button"
            disabled={!runtime.activeRuntime.activeTurnId}
            onClick={() => void runtime.cancelTurn()}
          >
            Stop
          </button>
          <button
            className="primary-button"
            type="button"
            disabled={!runtime.activeThread || !message.trim()}
            onClick={submit}
          >
            Send
          </button>
        </div>
      </form>
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
      <div className="setting-row">
        <span>Server URL</span>
        <strong>{preferences.serverUrl}</strong>
      </div>
      <div className="setting-row">
        <span>Density</span>
        <select
          value={preferences.density}
          onChange={(event) =>
            setPreferences((current) => ({ ...current, density: event.target.value as Preferences["density"] }))
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
            setPreferences((current) => ({ ...current, markdown: event.target.value as Preferences["markdown"] }))
          }
        >
          <option value="rendered">Rendered</option>
          <option value="plain">Plain</option>
        </select>
      </div>
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
      <section className="provider-form">
        <p className="eyebrow">Provider auth</p>
        <input value={providerId} onChange={(event) => setProviderId(event.target.value)} placeholder="Provider id" />
        <input value={profileId} onChange={(event) => setProfileId(event.target.value)} placeholder="Profile id" />
        <input value={displayName} onChange={(event) => setDisplayName(event.target.value)} placeholder="Display name" />
        <input value={secret} onChange={(event) => setSecret(event.target.value)} placeholder="API key or token" type="password" />
        <button className="primary-button" type="button" onClick={saveProvider}>
          Save profile
        </button>
      </section>
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
    case "chat":
      return "Chat workbench";
    case "memory":
      return "Memory";
    case "approvals":
      return "Approvals";
    case "channels":
      return "Channels";
    case "settings":
      return "Settings";
  }
}

function workspaceLabel(thread: Thread) {
  return thread.workspace_ref?.trim() ? thread.workspace_ref : "conversation-only";
}
