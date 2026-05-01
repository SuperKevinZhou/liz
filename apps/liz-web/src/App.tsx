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
import { loadPreferences, savePreferences, type Preferences } from "./preferences";

type ViewId = "chat" | "memory" | "approvals" | "channels" | "settings";

const views: Array<{ id: ViewId; label: string; icon: React.ComponentType<{ size?: number }> }> = [
  { id: "chat", label: "Chat", icon: MessageSquareText },
  { id: "memory", label: "Memory", icon: Brain },
  { id: "approvals", label: "Approvals", icon: CheckSquare },
  { id: "channels", label: "Channels", icon: PlugZap },
  { id: "settings", label: "Settings", icon: Settings },
];

const sampleThreads = [
  {
    id: "thread_local_console",
    title: "Liz Web Console",
    status: "active",
    summary: "Console shell ready for the protocol client.",
    workspace: "conversation-only",
  },
  {
    id: "thread_memory_review",
    title: "Memory Review",
    status: "waiting_approval",
    summary: "Compilation surface and evidence viewer pending.",
    workspace: "D:/zzh/Code/liz/liz",
  },
];

export function App() {
  const [activeView, setActiveView] = useState<ViewId>("chat");
  const [preferences, setPreferences] = useState<Preferences>(() => loadPreferences());

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
        <header className="panel-header">
          <div>
            <p className="eyebrow">Threads</p>
            <h1>Liz Console</h1>
          </div>
          <button className="icon-button" type="button" title="New thread" aria-label="New thread">
            <MessageSquareText size={17} />
          </button>
        </header>

        <label className="search-field">
          <Search size={15} />
          <input placeholder="Search threads" />
        </label>

        <div className="thread-list">
          {sampleThreads.map((thread, index) => (
            <button key={thread.id} className={index === 0 ? "thread-item active" : "thread-item"}>
              <span className={`status-dot ${thread.status}`} />
              <span>
                <strong>{thread.title}</strong>
                <small>{thread.summary}</small>
                <em>{thread.workspace}</em>
              </span>
            </button>
          ))}
        </div>

        <section className="side-section">
          <div className="section-row">
            <span>Workspace</span>
            <strong>Conversation only</strong>
          </div>
          <div className="section-row">
            <span>Channel</span>
            <strong>Web owner</strong>
          </div>
        </section>
      </aside>

      <main className="workspace">
        <TopBar activeView={activeView} preferences={preferences} setPreferences={setPreferences} />
        <WorkspaceView activeView={activeView} preferences={preferences} />
      </main>

      <aside className="inspector">
        <header className="panel-header compact">
          <div>
            <p className="eyebrow">Inspector</p>
            <h2>Selection</h2>
          </div>
          <button className="icon-button" type="button" title="Fork thread" aria-label="Fork thread">
            <GitFork size={16} />
          </button>
        </header>
        <div className="inspector-body">
          <section>
            <p className="eyebrow">Tool detail</p>
            <h3>No tool selected</h3>
            <p className="muted">
              Tool output, approvals, artifacts, diffs, and memory evidence appear here when selected.
            </p>
          </section>
          <section className="metric-grid">
            <div>
              <span>Pending approvals</span>
              <strong>0</strong>
            </div>
            <div>
              <span>Tool events</span>
              <strong>0</strong>
            </div>
          </section>
        </div>
      </aside>
    </div>
  );
}

function TopBar({
  activeView,
  preferences,
  setPreferences,
}: {
  activeView: ViewId;
  preferences: Preferences;
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
        <button className="primary-button" type="button">
          Connect
        </button>
      </div>
    </header>
  );
}

function WorkspaceView({ activeView, preferences }: { activeView: ViewId; preferences: Preferences }) {
  if (activeView === "chat") {
    return <ChatSurface />;
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

function ChatSurface() {
  return (
    <section className="chat-surface">
      <div className="transcript">
        <article className="message user">
          <span>User</span>
          <p>Build the first Liz web console.</p>
        </article>
        <article className="message assistant streaming">
          <span>Liz</span>
          <p>The console shell is ready. Protocol events will stream into this transcript.</p>
        </article>
        <article className="tool-line">
          <TerminalSquare size={16} />
          <div>
            <strong>Tool timeline</strong>
            <p>Tool calls will collapse into timeline rows and expand in the inspector.</p>
          </div>
        </article>
      </div>
      <form className="composer">
        <textarea placeholder="Message Liz" rows={3} />
        <div>
          <span>conversation-only</span>
          <button className="secondary-button" type="button">
            Stop
          </button>
          <button className="primary-button" type="button">
            Send
          </button>
        </div>
      </form>
    </section>
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
