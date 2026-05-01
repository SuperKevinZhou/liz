import type {
  AssistantChunkEventPayload,
  AssistantCompletedEventPayload,
  ServerEvent,
  Thread,
  ThreadEventPayload,
  ThreadId,
  ToolCallCommittedEventPayload,
  ToolCallStartedEventPayload,
  ToolCallUpdatedEventPayload,
  ToolCompletedEventPayload,
  ToolFailedEventPayload,
  Turn,
  TurnEventPayload,
  TurnFailedEventPayload,
  TurnId,
  ExecutorOutputChunkEventPayload,
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

export interface ToolOutputChunk {
  executorTaskId: string;
  stream: "stdout" | "stderr";
  chunk: string;
}

export interface ToolCallProjection {
  callId: string;
  threadId: ThreadId;
  turnId: TurnId | null;
  toolName: string;
  summary: string;
  status: "forming" | "committed" | "completed" | "failed";
  argumentsSummary: string | null;
  preview: string | null;
  riskHint: "low" | "medium" | "high" | "critical" | null;
  artifactIds: string[];
  output: ToolOutputChunk[];
  createdAt: string;
  updatedAt: string;
}

export interface WorkbenchState {
  threads: Thread[];
  activeThreadId: ThreadId | null;
  transcriptByThread: Record<ThreadId, TranscriptEntry[]>;
  runtimeByThread: Record<ThreadId, ThreadRuntime>;
  toolCallsByThread: Record<ThreadId, ToolCallProjection[]>;
  selectedToolCallId: string | null;
}

export type WorkbenchAction =
  | { type: "threads_loaded"; threads: Thread[] }
  | { type: "thread_upsert"; thread: Thread; activate?: boolean }
  | { type: "active_thread_set"; threadId: ThreadId | null }
  | { type: "user_message_added"; threadId: ThreadId; content: string; createdAt: string }
  | { type: "turn_started"; turn: Turn }
  | { type: "server_event"; event: ServerEvent }
  | { type: "resume_summary_added"; threadId: ThreadId; content: string; createdAt: string }
  | { type: "thread_error"; threadId: ThreadId; message: string }
  | { type: "tool_selected"; callId: string | null };

export const initialWorkbenchState: WorkbenchState = {
  threads: [],
  activeThreadId: null,
  transcriptByThread: {},
  runtimeByThread: {},
  toolCallsByThread: {},
  selectedToolCallId: null,
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

    case "tool_selected":
      return {
        ...state,
        selectedToolCallId: action.callId,
      };
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

export const activeToolCalls = (state: WorkbenchState) =>
  state.activeThreadId ? (state.toolCallsByThread[state.activeThreadId] ?? []) : [];

export const selectedToolCall = (state: WorkbenchState) => {
  const calls = Object.values(state.toolCallsByThread).flat();
  return calls.find((call) => call.callId === state.selectedToolCallId) ?? null;
};

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

    case "tool_call_started":
      return upsertToolCall(state, event.thread_id, {
        callId: (event.payload as ToolCallStartedEventPayload).call_id,
        threadId: event.thread_id,
        turnId: event.turn_id,
        toolName: (event.payload as ToolCallStartedEventPayload).tool_name,
        summary: (event.payload as ToolCallStartedEventPayload).summary,
        status: "forming",
        argumentsSummary: null,
        preview: null,
        riskHint: null,
        artifactIds: [],
        output: [],
        createdAt: event.created_at,
        updatedAt: event.created_at,
      });

    case "tool_call_updated": {
      const payload = event.payload as ToolCallUpdatedEventPayload;
      return updateToolCall(state, event.thread_id, payload.call_id, (call) => ({
        ...call,
        toolName: payload.tool_name,
        summary: payload.delta_summary,
        preview: payload.preview,
        updatedAt: event.created_at,
      }));
    }

    case "tool_call_committed": {
      const payload = event.payload as ToolCallCommittedEventPayload;
      return updateToolCall(state, event.thread_id, payload.call_id, (call) => ({
        ...call,
        toolName: payload.tool_name,
        status: "committed",
        argumentsSummary: payload.arguments_summary,
        riskHint: payload.risk_hint,
        updatedAt: event.created_at,
      }));
    }

    case "tool_completed": {
      const payload = event.payload as ToolCompletedEventPayload;
      const callId = resolveToolCallId(state, event.thread_id, event.turn_id, payload.tool_name);
      return updateToolCall(state, event.thread_id, callId, (call) => ({
        ...call,
        status: "completed",
        summary: payload.summary,
        artifactIds: payload.artifact_ids,
        updatedAt: event.created_at,
      }));
    }

    case "tool_failed": {
      const payload = event.payload as ToolFailedEventPayload;
      const callId = resolveToolCallId(state, event.thread_id, event.turn_id, payload.tool_name);
      return updateToolCall(state, event.thread_id, callId, (call) => ({
        ...call,
        status: "failed",
        summary: payload.summary,
        updatedAt: event.created_at,
      }));
    }

    case "executor_output_chunk": {
      const payload = event.payload as ExecutorOutputChunkEventPayload;
      const callId = resolveLatestToolCallId(state, event.thread_id, event.turn_id);
      if (!callId) {
        return state;
      }
      return updateToolCall(state, event.thread_id, callId, (call) => ({
        ...call,
        output: [
          ...call.output,
          {
            executorTaskId: payload.executor_task_id,
            stream: payload.stream,
            chunk: payload.chunk,
          },
        ],
        updatedAt: event.created_at,
      }));
    }

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

const upsertToolCall = (
  state: WorkbenchState,
  threadId: ThreadId,
  toolCall: ToolCallProjection,
): WorkbenchState => {
  const calls = state.toolCallsByThread[threadId] ?? [];
  return {
    ...state,
    selectedToolCallId: state.selectedToolCallId ?? toolCall.callId,
    toolCallsByThread: {
      ...state.toolCallsByThread,
      [threadId]: [
        ...calls.filter((call) => call.callId !== toolCall.callId),
        calls.find((call) => call.callId === toolCall.callId)
          ? { ...calls.find((call) => call.callId === toolCall.callId)!, ...toolCall }
          : toolCall,
      ].sort((left, right) => left.createdAt.localeCompare(right.createdAt)),
    },
  };
};

const updateToolCall = (
  state: WorkbenchState,
  threadId: ThreadId,
  callId: string | null,
  update: (toolCall: ToolCallProjection) => ToolCallProjection,
): WorkbenchState => {
  if (!callId) {
    return state;
  }

  const calls = state.toolCallsByThread[threadId] ?? [];
  return {
    ...state,
    toolCallsByThread: {
      ...state.toolCallsByThread,
      [threadId]: calls.map((call) => (call.callId === callId ? update(call) : call)),
    },
  };
};

const resolveToolCallId = (
  state: WorkbenchState,
  threadId: ThreadId,
  turnId: TurnId | null,
  toolName: string,
) => {
  const calls = state.toolCallsByThread[threadId] ?? [];
  return [...calls]
    .reverse()
    .find(
      (call) =>
        call.toolName === toolName &&
        call.turnId === turnId &&
        (call.status === "forming" || call.status === "committed"),
    )?.callId ?? null;
};

const resolveLatestToolCallId = (
  state: WorkbenchState,
  threadId: ThreadId,
  turnId: TurnId | null,
) => {
  const calls = state.toolCallsByThread[threadId] ?? [];
  return [...calls]
    .reverse()
    .find(
      (call) =>
        call.turnId === turnId && (call.status === "forming" || call.status === "committed"),
    )?.callId ?? null;
};
