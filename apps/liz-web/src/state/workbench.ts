import type {
  AssistantChunkEventPayload,
  AssistantCompletedEventPayload,
  ServerEvent,
  Thread,
  ThreadEventPayload,
  ThreadId,
  Turn,
  TurnEventPayload,
  TurnFailedEventPayload,
  TurnId,
} from "../protocol/types";

export type TranscriptEntry =
  | {
      id: string;
      kind: "user";
      threadId: ThreadId;
      turnId: TurnId | null;
      content: string;
      createdAt: string;
      status: "sent" | "pending" | "failed";
    }
  | {
      id: string;
      kind: "assistant";
      threadId: ThreadId;
      turnId: TurnId | null;
      content: string;
      createdAt: string;
      status: "streaming" | "completed" | "cancelled" | "failed";
    }
  | {
      id: string;
      kind: "system";
      threadId: ThreadId;
      turnId: TurnId | null;
      content: string;
      createdAt: string;
      tone: "info" | "error";
    };

export interface ThreadRuntime {
  activeTurnId: TurnId | null;
  lastError: string | null;
}

export interface WorkbenchState {
  threads: Thread[];
  activeThreadId: ThreadId | null;
  transcriptByThread: Record<ThreadId, TranscriptEntry[]>;
  runtimeByThread: Record<ThreadId, ThreadRuntime>;
}

export type WorkbenchAction =
  | { type: "threads_loaded"; threads: Thread[] }
  | { type: "thread_upsert"; thread: Thread; activate?: boolean }
  | { type: "active_thread_set"; threadId: ThreadId | null }
  | { type: "user_message_added"; threadId: ThreadId; content: string; createdAt: string }
  | { type: "turn_started"; turn: Turn }
  | { type: "server_event"; event: ServerEvent }
  | { type: "resume_summary_added"; threadId: ThreadId; content: string; createdAt: string }
  | { type: "thread_error"; threadId: ThreadId; message: string };

export const initialWorkbenchState: WorkbenchState = {
  threads: [],
  activeThreadId: null,
  transcriptByThread: {},
  runtimeByThread: {},
};

export const workbenchReducer = (
  state: WorkbenchState,
  action: WorkbenchAction,
): WorkbenchState => {
  switch (action.type) {
    case "threads_loaded": {
      const activeThreadId =
        state.activeThreadId && action.threads.some((thread) => thread.id === state.activeThreadId)
          ? state.activeThreadId
          : (action.threads[0]?.id ?? null);

      return {
        ...state,
        activeThreadId,
        threads: sortThreads(action.threads),
      };
    }

    case "thread_upsert":
      return {
        ...state,
        activeThreadId: action.activate ? action.thread.id : state.activeThreadId,
        threads: upsertThread(state.threads, action.thread),
      };

    case "active_thread_set":
      return {
        ...state,
        activeThreadId: action.threadId,
      };

    case "user_message_added":
      return appendEntry(state, action.threadId, {
        id: `user:${action.createdAt}:${state.transcriptByThread[action.threadId]?.length ?? 0}`,
        kind: "user",
        threadId: action.threadId,
        turnId: null,
        content: action.content,
        createdAt: action.createdAt,
        status: "pending",
      });

    case "turn_started": {
      const threadId = action.turn.thread_id;
      const entries = state.transcriptByThread[threadId] ?? [];
      const nextEntries = entries.map((entry) =>
        entry.kind === "user" && entry.status === "pending"
          ? { ...entry, turnId: action.turn.id, status: "sent" as const }
          : entry,
      );

      return {
        ...state,
        transcriptByThread: {
          ...state.transcriptByThread,
          [threadId]: ensureAssistantEntry(nextEntries, threadId, action.turn.id, action.turn.started_at),
        },
        runtimeByThread: {
          ...state.runtimeByThread,
          [threadId]: { activeTurnId: action.turn.id, lastError: null },
        },
      };
    }

    case "server_event":
      return projectServerEvent(state, action.event);

    case "resume_summary_added":
      return appendEntry(state, action.threadId, {
        id: `resume:${action.createdAt}`,
        kind: "system",
        threadId: action.threadId,
        turnId: null,
        content: action.content,
        createdAt: action.createdAt,
        tone: "info",
      });

    case "thread_error":
      return appendEntry(
        {
          ...state,
          runtimeByThread: {
            ...state.runtimeByThread,
            [action.threadId]: {
              activeTurnId: state.runtimeByThread[action.threadId]?.activeTurnId ?? null,
              lastError: action.message,
            },
          },
        },
        action.threadId,
        {
          id: `error:${Date.now()}`,
          kind: "system",
          threadId: action.threadId,
          turnId: null,
          content: action.message,
          createdAt: new Date().toISOString(),
          tone: "error",
        },
      );
  }
};

export const activeThread = (state: WorkbenchState) =>
  state.threads.find((thread) => thread.id === state.activeThreadId) ?? null;

export const activeTranscript = (state: WorkbenchState) =>
  state.activeThreadId ? (state.transcriptByThread[state.activeThreadId] ?? []) : [];

export const activeRuntime = (state: WorkbenchState) =>
  state.activeThreadId
    ? (state.runtimeByThread[state.activeThreadId] ?? { activeTurnId: null, lastError: null })
    : { activeTurnId: null, lastError: null };

const projectServerEvent = (state: WorkbenchState, event: ServerEvent): WorkbenchState => {
  switch (event.event_type) {
    case "thread_started":
    case "thread_resumed":
    case "thread_forked":
    case "thread_updated":
    case "thread_interrupted":
    case "thread_archived":
      return workbenchReducer(state, {
        type: "thread_upsert",
        thread: (event.payload as ThreadEventPayload).thread,
      });

    case "turn_started":
      return workbenchReducer(state, {
        type: "turn_started",
        turn: (event.payload as TurnEventPayload).turn,
      });

    case "assistant_chunk":
      return appendAssistantChunk(state, event);

    case "assistant_completed":
      return completeAssistantMessage(state, event, (event.payload as AssistantCompletedEventPayload).message);

    case "turn_completed":
      return finishTurn(state, (event.payload as TurnEventPayload).turn, "completed");

    case "turn_cancelled":
      return finishTurn(state, (event.payload as TurnEventPayload).turn, "cancelled");

    case "turn_failed":
      return failTurn(state, event.payload as TurnFailedEventPayload);

    default:
      return state;
  }
};

const appendAssistantChunk = (state: WorkbenchState, event: ServerEvent) => {
  const payload = event.payload as AssistantChunkEventPayload;
  const threadId = event.thread_id;
  const turnId = event.turn_id;
  const entries = ensureAssistantEntry(
    state.transcriptByThread[threadId] ?? [],
    threadId,
    turnId,
    event.created_at,
  );
  const nextEntries = entries.map((entry) =>
    entry.kind === "assistant" && entry.turnId === turnId
      ? {
          ...entry,
          content: `${entry.content}${payload.chunk}`,
          status: payload.is_final ? ("completed" as const) : entry.status,
        }
      : entry,
  );

  return {
    ...state,
    transcriptByThread: {
      ...state.transcriptByThread,
      [threadId]: nextEntries,
    },
  };
};

const completeAssistantMessage = (state: WorkbenchState, event: ServerEvent, message: string) => {
  const threadId = event.thread_id;
  const turnId = event.turn_id;
  const entries = ensureAssistantEntry(
    state.transcriptByThread[threadId] ?? [],
    threadId,
    turnId,
    event.created_at,
  );

  return {
    ...state,
    transcriptByThread: {
      ...state.transcriptByThread,
      [threadId]: entries.map((entry) =>
        entry.kind === "assistant" && entry.turnId === turnId
          ? { ...entry, content: message || entry.content, status: "completed" as const }
          : entry,
      ),
    },
  };
};

const finishTurn = (
  state: WorkbenchState,
  turn: Turn,
  status: Extract<TranscriptEntry, { kind: "assistant" }>["status"],
) => ({
  ...state,
  transcriptByThread: {
    ...state.transcriptByThread,
    [turn.thread_id]: (state.transcriptByThread[turn.thread_id] ?? []).map((entry) =>
      entry.kind === "assistant" && entry.turnId === turn.id ? { ...entry, status } : entry,
    ),
  },
  runtimeByThread: {
    ...state.runtimeByThread,
    [turn.thread_id]: { activeTurnId: null, lastError: null },
  },
});

const failTurn = (state: WorkbenchState, payload: TurnFailedEventPayload) => {
  const failed = finishTurn(state, payload.turn, "failed");
  return appendEntry(failed, payload.turn.thread_id, {
    id: `turn-failed:${payload.turn.id}`,
    kind: "system",
    threadId: payload.turn.thread_id,
    turnId: payload.turn.id,
    content: payload.message,
    createdAt: payload.turn.ended_at ?? new Date().toISOString(),
    tone: "error",
  });
};

const ensureAssistantEntry = (
  entries: TranscriptEntry[],
  threadId: ThreadId,
  turnId: TurnId | null,
  createdAt: string,
) => {
  if (entries.some((entry) => entry.kind === "assistant" && entry.turnId === turnId)) {
    return entries;
  }

  return [
    ...entries,
    {
      id: `assistant:${turnId ?? createdAt}`,
      kind: "assistant" as const,
      threadId,
      turnId,
      content: "",
      createdAt,
      status: "streaming" as const,
    },
  ];
};

const appendEntry = (
  state: WorkbenchState,
  threadId: ThreadId,
  entry: TranscriptEntry,
): WorkbenchState => ({
  ...state,
  transcriptByThread: {
    ...state.transcriptByThread,
    [threadId]: [...(state.transcriptByThread[threadId] ?? []), entry],
  },
});

const upsertThread = (threads: Thread[], thread: Thread) =>
  sortThreads([thread, ...threads.filter((current) => current.id !== thread.id)]);

const sortThreads = (threads: Thread[]) =>
  [...threads].sort((left, right) => right.updated_at.localeCompare(left.updated_at));
