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
import type { Thread, ThreadId } from "./protocol/types";
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
        <div className="rail-status" title="Disconnected">
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
        <WorkspaceView activeView={activeView} preferences={preferences} runtime={runtime} />
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
}: {
  activeView: ViewId;
  preferences: Preferences;
  runtime: LizRuntime;
}) {
  if (activeView === "chat") {
    return <ChatSurface runtime={runtime} />;
  }

  if (activeView === "settings") {
    return <SettingsSurface preferences={preferences} />;
  }

  const panels = {
    memory: {
      icon: Brain,
      title: "Memory",
      rows: ["Wakeup", "Compile now", "Search", "Evidence"],
    },
    approvals: {
      icon: ShieldCheck,
      title: "Approvals",
      rows: ["Pending", "Resolved", "Risk", "Decision"],
    },
    channels: {
      icon: PlugZap,
      title: "Channels",
      rows: ["Telegram", "Discord", "Email", "Unknown"],
    },
  } as const;
  const panel = panels[activeView as keyof typeof panels];
  const Icon = panel.icon;

  return (
    <section className="placeholder-surface">
      <Icon size={22} />
      <h2>{panel.title}</h2>
      <div className="lined-list">
        {panel.rows.map((row) => (
          <button key={row}>
            <span>{row}</span>
            <small>Not loaded</small>
          </button>
        ))}
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
          <button className="secondary-button" type="button" onClick={() => void runtime.cancelTurn()}>
            Stop
          </button>
          <button className="primary-button" type="button" onClick={submit}>
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

function SettingsSurface({ preferences }: { preferences: Preferences }) {
  return (
    <section className="settings-surface">
      <div className="setting-row">
        <span>Server URL</span>
        <strong>{preferences.serverUrl}</strong>
      </div>
      <div className="setting-row">
        <span>Density</span>
        <strong>{preferences.density}</strong>
      </div>
      <div className="setting-row">
        <span>Markdown</span>
        <strong>{preferences.markdown}</strong>
      </div>
      <div className="setting-row">
        <span>Tool cards</span>
        <strong>{preferences.toolVerbosity}</strong>
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
            <span>Messages</span>
            <strong>{runtime.activeToolCalls.length}</strong>
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
