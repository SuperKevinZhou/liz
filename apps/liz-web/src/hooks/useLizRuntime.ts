import { useCallback, useMemo, useReducer, useRef, useState } from "react";
import { createProtocolClient, type LizProtocolClient } from "../protocol/client";
import type {
  ApprovalRespondRequest,
  ApprovalRespondResponse,
  ConnectionState,
  MemoryCompileNowRequest,
  MemoryCompileNowResponse,
  MemoryListTopicsRequest,
  MemoryListTopicsResponse,
  MemoryOpenEvidenceRequest,
  MemoryOpenEvidenceResponse,
  MemoryOpenSessionRequest,
  MemoryOpenSessionResponse,
  MemoryReadWakeupRequest,
  MemoryReadWakeupResponse,
  MemorySearchRequest,
  MemorySearchResponse,
  ModelStatusResponse,
  ProviderAuthDeleteRequest,
  ProviderAuthDeleteResponse,
  ProviderAuthListRequest,
  ProviderAuthListResponse,
  ProviderAuthProfile,
  ProviderAuthUpsertRequest,
  ProviderAuthUpsertResponse,
  ResponseError,
  RuntimeConfigResponse,
  RuntimeConfigUpdateRequest,
  Thread,
  ThreadForkRequest,
  ThreadId,
  ThreadListRequest,
  ThreadListResponse,
  ThreadResumeRequest,
  ThreadResumeResponse,
  ThreadStartRequest,
  ThreadStartResponse,
  TurnCancelRequest,
  TurnCancelResponse,
  TurnStartRequest,
  TurnStartResponse,
} from "../protocol/types";
import {
  activeApprovals,
  activeMemory,
  activeResumePanel,
  activeRuntime,
  activeThread,
  activeToolCalls,
  activeTranscript,
  allApprovals,
  initialWorkbenchState,
  selectedToolCall,
  workbenchReducer,
} from "../state/workbench";
import type { Preferences } from "../preferences";

export interface LizRuntime {
  connectionState: ConnectionState;
  error: string | null;
  state: typeof initialWorkbenchState;
  activeThread: Thread | null;
  activeTranscript: ReturnType<typeof activeTranscript>;
  activeRuntime: ReturnType<typeof activeRuntime>;
  activeToolCalls: ReturnType<typeof activeToolCalls>;
  activeApprovals: ReturnType<typeof activeApprovals>;
  allApprovals: ReturnType<typeof allApprovals>;
  activeMemory: ReturnType<typeof activeMemory>;
  activeResumePanel: ReturnType<typeof activeResumePanel>;
  selectedToolCall: ReturnType<typeof selectedToolCall>;
  selectToolCall: (callId: string | null) => void;
  connect: () => void;
  close: () => void;
  refreshThreads: () => Promise<void>;
  setActiveThread: (threadId: ThreadId) => Promise<void>;
  startThread: (request: ThreadStartRequest) => Promise<void>;
  forkThread: (request: ThreadForkRequest) => Promise<void>;
  startTurn: (input: string) => Promise<void>;
  cancelTurn: () => Promise<void>;
  respondToApproval: (request: ApprovalRespondRequest) => Promise<void>;
  readMemoryWakeup: () => Promise<void>;
  compileMemory: () => Promise<void>;
  listMemoryTopics: () => Promise<void>;
  searchMemory: (query: string, mode: "keyword" | "semantic") => Promise<void>;
  openMemoryEvidence: (request: MemoryOpenEvidenceRequest) => Promise<void>;
  loadRuntimeState: () => Promise<void>;
  updateRuntimeConfig: (request: RuntimeConfigUpdateRequest) => Promise<void>;
  upsertProviderProfile: (profile: ProviderAuthProfile) => Promise<void>;
  deleteProviderProfile: (profileId: string) => Promise<void>;
}

export const useLizRuntime = (preferences: Preferences): LizRuntime => {
  const [state, dispatch] = useReducer(workbenchReducer, initialWorkbenchState);
  const [connectionState, setConnectionState] = useState<ConnectionState>("idle");
  const [error, setError] = useState<string | null>(null);
  const clientRef = useRef<LizProtocolClient | null>(null);

  const ensureClient = useCallback(() => {
    if (clientRef.current) {
      return clientRef.current;
    }

    const client = createProtocolClient();
    client.onState((nextState) => {
      setConnectionState(nextState);
      if (nextState === "connected") {
        void requestThreadList(client, dispatch, setError);
      }
    });
    client.onEvent((event) => dispatch({ type: "server_event", event }));
    client.onUnknown(() => {
      setError("Received an unsupported server frame.");
    });
    clientRef.current = client;
    return client;
  }, []);

  const request = useCallback(
    async <Data, Params>(method: Parameters<LizProtocolClient["request"]>[0], params: Params) => {
      const client = ensureClient();
      const response = await client.request<Data, Params>(method, params);
      if (!response.ok) {
        throw response.error;
      }
      return response;
    },
    [ensureClient],
  );

  const currentThread = activeThread(state);
  const transcript = activeTranscript(state);
  const runtime = activeRuntime(state);
  const toolCalls = activeToolCalls(state);
  const selectedTool = selectedToolCall(state);
  const approvals = activeApprovals(state);
  const everyApproval = allApprovals(state);
  const memory = activeMemory(state);
  const resumePanel = activeResumePanel(state);

  const connect = useCallback(() => {
    setError(null);
    ensureClient().connect(preferences.serverUrl);
  }, [ensureClient, preferences.serverUrl]);

  const close = useCallback(() => {
    clientRef.current?.close();
  }, []);

  const refreshThreads = useCallback(async () => {
    await requestThreadList(ensureClient(), dispatch, setError);
  }, [ensureClient]);

  const setActiveThread = useCallback(
    async (threadId: ThreadId) => {
      dispatch({ type: "active_thread_set", threadId });
      try {
        const response = await request<ThreadResumeResponse, ThreadResumeRequest>("thread/resume", {
          thread_id: threadId,
        });
        dispatch({ type: "thread_upsert", thread: response.data.thread, activate: true });
        const summary = response.data.resume_summary;
        if (summary) {
          dispatch({
            type: "resume_summary_set",
            threadId,
            headline: summary.headline,
            activeSummary: summary.active_summary,
            pendingCommitments: summary.pending_commitments,
            lastInterruption: summary.last_interruption,
          });
        }
        void loadThreadSession(threadId, request, dispatch, setError);
      } catch (caught) {
        setError(messageFromError(caught));
      }
    },
    [request],
  );

  const startThread = useCallback(
    async (threadRequest: ThreadStartRequest) => {
      try {
        const response = await request<ThreadStartResponse, ThreadStartRequest>(
          "thread/start",
          threadRequest,
        );
        dispatch({ type: "thread_upsert", thread: response.data.thread, activate: true });
      } catch (caught) {
        setError(messageFromError(caught));
      }
    },
    [request],
  );

  const forkThread = useCallback(
    async (forkRequest: ThreadForkRequest) => {
      try {
        const response = await request<{ thread: Thread }, ThreadForkRequest>(
          "thread/fork",
          forkRequest,
        );
        dispatch({ type: "thread_upsert", thread: response.data.thread, activate: true });
      } catch (caught) {
        setError(messageFromError(caught));
      }
    },
    [request],
  );

  const startTurn = useCallback(
    async (input: string) => {
      const thread = activeThread(state);
      if (!thread || !input.trim()) {
        return;
      }

      dispatch({
        type: "user_message_added",
        threadId: thread.id,
        content: input.trim(),
        createdAt: new Date().toISOString(),
      });

      const requestPayload: TurnStartRequest = {
        thread_id: thread.id,
        input: input.trim(),
        input_kind: "user_message",
        channel: {
          kind: "web",
          external_conversation_id: `web:${preferences.browserInstanceId}:${thread.id}`,
        },
        participant: {
          external_participant_id: "owner",
          display_name: "Owner",
        },
      };

      try {
        const response = await request<TurnStartResponse, TurnStartRequest>(
          "turn/start",
          requestPayload,
        );
        dispatch({ type: "turn_started", turn: response.data.turn });
      } catch (caught) {
        dispatch({ type: "thread_error", threadId: thread.id, message: messageFromError(caught) });
      }
    },
    [preferences.browserInstanceId, request, state],
  );

  const cancelTurn = useCallback(async () => {
    const thread = activeThread(state);
    const activeTurnId = activeRuntime(state).activeTurnId;
    if (!thread || !activeTurnId) {
      return;
    }

    try {
      const response = await request<TurnCancelResponse, TurnCancelRequest>("turn/cancel", {
        thread_id: thread.id,
        turn_id: activeTurnId,
      });
      dispatch({
        type: "server_event",
        event: {
          event_id: `local-cancel:${activeTurnId}`,
          thread_id: thread.id,
          turn_id: activeTurnId,
          created_at: new Date().toISOString(),
          event_type: "turn_cancelled",
          payload: { turn: response.data.turn },
        },
      });
    } catch (caught) {
      dispatch({ type: "thread_error", threadId: thread.id, message: messageFromError(caught) });
    }
  }, [request, state]);

  const respondToApproval = useCallback(
    async (approvalRequest: ApprovalRespondRequest) => {
      const response = await request<ApprovalRespondResponse, ApprovalRespondRequest>(
        "approval/respond",
        approvalRequest,
      );
      dispatch({ type: "approval_upsert", approval: response.data.approval });
    },
    [request],
  );

  const selectTool = useCallback((callId: string | null) => {
    dispatch({ type: "tool_selected", callId });
  }, []);

  const readMemoryWakeup = useCallback(async () => {
    const thread = activeThread(state);
    if (!thread) {
      return;
    }
    const response = await request<MemoryReadWakeupResponse, MemoryReadWakeupRequest>(
      "memory/read_wakeup",
      { thread_id: thread.id },
    );
    dispatch({
      type: "memory_wakeup_set",
      threadId: response.data.thread_id,
      wakeup: response.data.wakeup,
      recentConversation: response.data.recent_conversation,
    });
  }, [request, state]);

  const compileMemory = useCallback(async () => {
    const thread = activeThread(state);
    if (!thread) {
      return;
    }
    const response = await request<MemoryCompileNowResponse, MemoryCompileNowRequest>(
      "memory/compile_now",
      { thread_id: thread.id },
    );
    dispatch({
      type: "memory_compilation_set",
      threadId: response.data.thread_id,
      compilation: response.data.compilation,
    });
  }, [request, state]);

  const listMemoryTopics = useCallback(async () => {
    const response = await request<MemoryListTopicsResponse, MemoryListTopicsRequest>(
      "memory/list_topics",
      { status: null, limit: 80 },
    );
    dispatch({ type: "memory_topics_set", topics: response.data.topics });
  }, [request]);

  const searchMemory = useCallback(
    async (query: string, mode: "keyword" | "semantic") => {
      if (!query.trim()) {
        return;
      }
      const response = await request<MemorySearchResponse, MemorySearchRequest>("memory/search", {
        query: query.trim(),
        mode,
        limit: 20,
      });
      dispatch({
        type: "memory_search_set",
        query: response.data.query,
        mode: response.data.mode,
        hits: response.data.hits,
      });
    },
    [request],
  );

  const openMemoryEvidence = useCallback(
    async (evidenceRequest: MemoryOpenEvidenceRequest) => {
      const response = await request<MemoryOpenEvidenceResponse, MemoryOpenEvidenceRequest>(
        "memory/open_evidence",
        evidenceRequest,
      );
      dispatch({ type: "memory_evidence_set", evidence: response.data.evidence });
    },
    [request],
  );

  const loadRuntimeState = useCallback(async () => {
    const [runtimeConfig, providers, model] = await Promise.all([
      request<RuntimeConfigResponse, Record<string, never>>("runtime/config_get", {}),
      request<ProviderAuthListResponse, ProviderAuthListRequest>("provider_auth/list", {
        provider_id: null,
      }),
      request<ModelStatusResponse, Record<string, never>>("model/status", {}),
    ]);
    dispatch({ type: "runtime_config_set", config: runtimeConfig.data });
    dispatch({ type: "provider_profiles_set", profiles: providers.data.profiles });
    dispatch({ type: "model_status_set", status: model.data });
  }, [request]);

  const updateRuntimeConfig = useCallback(
    async (configRequest: RuntimeConfigUpdateRequest) => {
      const response = await request<RuntimeConfigResponse, RuntimeConfigUpdateRequest>(
        "runtime/config_update",
        configRequest,
      );
      dispatch({ type: "runtime_config_set", config: response.data });
    },
    [request],
  );

  const upsertProviderProfile = useCallback(
    async (profile: ProviderAuthProfile) => {
      const response = await request<ProviderAuthUpsertResponse, ProviderAuthUpsertRequest>(
        "provider_auth/upsert",
        { profile },
      );
      dispatch({ type: "provider_profile_upsert", profile: response.data.profile });
    },
    [request],
  );

  const deleteProviderProfile = useCallback(
    async (profileId: string) => {
      const response = await request<ProviderAuthDeleteResponse, ProviderAuthDeleteRequest>(
        "provider_auth/delete",
        { profile_id: profileId },
      );
      dispatch({ type: "provider_profile_deleted", profileId: response.data.profile_id });
    },
    [request],
  );

  return useMemo(
    () => ({
      connectionState,
      error,
      state,
      activeThread: currentThread,
      activeTranscript: transcript,
      activeRuntime: runtime,
      activeToolCalls: toolCalls,
      activeApprovals: approvals,
      allApprovals: everyApproval,
      activeMemory: memory,
      activeResumePanel: resumePanel,
      selectedToolCall: selectedTool,
      selectToolCall: selectTool,
      connect,
      close,
      refreshThreads,
      setActiveThread,
      startThread,
      forkThread,
      startTurn,
      cancelTurn,
      respondToApproval,
      readMemoryWakeup,
      compileMemory,
      listMemoryTopics,
      searchMemory,
      openMemoryEvidence,
      loadRuntimeState,
      updateRuntimeConfig,
      upsertProviderProfile,
      deleteProviderProfile,
    }),
    [
      cancelTurn,
      close,
      compileMemory,
      connect,
      connectionState,
      currentThread,
      error,
      everyApproval,
      forkThread,
      deleteProviderProfile,
      listMemoryTopics,
      loadRuntimeState,
      memory,
      openMemoryEvidence,
      readMemoryWakeup,
      resumePanel,
      refreshThreads,
      respondToApproval,
      searchMemory,
      updateRuntimeConfig,
      upsertProviderProfile,
      approvals,
      runtime,
      selectedTool,
      selectTool,
      setActiveThread,
      startThread,
      startTurn,
      state,
      transcript,
      toolCalls,
    ],
  );
};

const requestThreadList = async (
  client: LizProtocolClient,
  dispatch: React.Dispatch<any>,
  setError: (error: string | null) => void,
) => {
  try {
    const response = await client.request<ThreadListResponse, ThreadListRequest>("thread/list", {
      status: null,
      limit: 100,
    });
    if (response.ok) {
      dispatch({ type: "threads_loaded", threads: response.data.threads });
    } else {
      setError(response.error.message);
    }
  } catch (caught) {
    setError(messageFromError(caught));
  }
};

type RuntimeRequest = <Data, Params>(
  method: Parameters<LizProtocolClient["request"]>[0],
  params: Params,
) => Promise<{ data: Data }>;

const loadThreadSession = async (
  threadId: ThreadId,
  request: RuntimeRequest,
  dispatch: React.Dispatch<any>,
  setError: (error: string | null) => void,
) => {
  try {
    const response = await request<MemoryOpenSessionResponse, MemoryOpenSessionRequest>(
      "memory/open_session",
      { thread_id: threadId },
    );
    dispatch({ type: "session_loaded", session: response.data.session });
  } catch (caught) {
    setError(messageFromError(caught));
  }
};

const messageFromError = (caught: unknown) => {
  const error = caught as Partial<ResponseError>;
  return error.message ?? "The Liz app server request failed.";
};
