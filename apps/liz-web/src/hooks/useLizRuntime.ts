import { useCallback, useMemo, useReducer, useRef, useState } from "react";
import { createProtocolClient, type LizProtocolClient } from "../protocol/client";
import type {
  ApprovalRespondRequest,
  ApprovalRespondResponse,
  AboutYouUpdate,
  MemorySurfaceAboutYouReadResponse,
  MemorySurfaceAboutYouUpdateResponse,
  MemorySurfaceCarryingReadResponse,
  MemorySurfaceKnowledgeCorrectResponse,
  MemorySurfaceKnowledgeListResponse,
  ConnectionState,
  KnowledgeCorrection,
  PeopleSurfaceDeleteRequest,
  PeopleSurfaceDeleteResponse,
  PeopleSurfaceReadResponse,
  PeopleSurfaceUpsertRequest,
  PeopleSurfaceUpsertResponse,
  PersonBoundary,
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
  NodeListResponse,
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
  WorkspaceMountListResponse,
  WorkspaceMountAttachRequest,
  WorkspaceMountAttachResponse,
  WorkspaceMountDetachRequest,
  WorkspaceMountDetachResponse,
  WorkspaceMountId,
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
  startTurn: (input: string, inputKind?: TurnStartRequest["input_kind"]) => Promise<void>;
  cancelTurn: () => Promise<void>;
  respondToApproval: (request: ApprovalRespondRequest) => Promise<void>;
  readMemoryWakeup: () => Promise<void>;
  compileMemory: () => Promise<void>;
  listMemoryTopics: () => Promise<void>;
  searchMemory: (query: string, mode: "keyword" | "semantic") => Promise<void>;
  openMemoryEvidence: (request: MemoryOpenEvidenceRequest) => Promise<void>;
  loadRuntimeState: () => Promise<void>;
  loadOwnerSurfaces: () => Promise<void>;
  loadPeopleSurface: () => Promise<void>;
  updateAboutYou: (update: AboutYouUpdate) => Promise<void>;
  correctKnowledge: (correction: KnowledgeCorrection) => Promise<void>;
  upsertPersonBoundary: (person: PersonBoundary) => Promise<void>;
  deletePersonBoundary: (personId: string) => Promise<void>;
  updateRuntimeConfig: (request: RuntimeConfigUpdateRequest) => Promise<void>;
  upsertProviderProfile: (profile: ProviderAuthProfile) => Promise<void>;
  deleteProviderProfile: (profileId: string) => Promise<void>;
  attachWorkspaceMount: (request: WorkspaceMountAttachRequest) => Promise<void>;
  detachWorkspaceMount: (workspaceId: WorkspaceMountId) => Promise<void>;
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
        void loadOwnerSurfacesWithClient(client, dispatch, setError);
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
    async (input: string, inputKind: TurnStartRequest["input_kind"] = "user_message") => {
      let thread = activeThread(state);
      if (!input.trim()) {
        return;
      }

      if (!thread) {
        const started = await request<ThreadStartResponse, ThreadStartRequest>("thread/start", {
          title: input.trim().slice(0, 48) || null,
          initial_goal: input.trim(),
          workspace_ref: null,
          workspace_mount_id: null,
        });
        thread = started.data.thread;
        dispatch({ type: "thread_upsert", thread, activate: true });
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
        input_kind: inputKind,
        channel: {
          kind: "web",
          external_conversation_id: `web:${preferences.browserInstanceId}:${thread.id}`,
        },
        participant: {
          external_participant_id: "owner",
          display_name: "Owner",
        },
        interaction_context: ownerInteractionContext(preferences.browserInstanceId, thread.id),
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
    const [runtimeConfig, providers, model, nodes, mounts] = await Promise.all([
      request<RuntimeConfigResponse, Record<string, never>>("runtime/config_get", {}),
      request<ProviderAuthListResponse, ProviderAuthListRequest>("provider_auth/list", {
        provider_id: null,
      }),
      request<ModelStatusResponse, Record<string, never>>("model/status", {}),
      request<NodeListResponse, Record<string, never>>("node/list", {}),
      request<WorkspaceMountListResponse, { node_id: null }>("workspace_mount/list", {
        node_id: null,
      }),
    ]);
    dispatch({ type: "runtime_config_set", config: runtimeConfig.data });
    dispatch({ type: "provider_profiles_set", profiles: providers.data.profiles });
    dispatch({ type: "model_status_set", status: model.data });
    dispatch({ type: "nodes_set", nodes: nodes.data.nodes });
    dispatch({ type: "workspace_mounts_set", mounts: mounts.data.mounts });
  }, [request]);

  const loadOwnerSurfaces = useCallback(async () => {
    const [aboutYou, carrying, knowledge] = await Promise.all([
      request<MemorySurfaceAboutYouReadResponse, Record<string, never>>(
        "memory_surface/about_you/read",
        {},
      ),
      request<MemorySurfaceCarryingReadResponse, { limit: number | null }>(
        "memory_surface/carrying/read",
        { limit: 20 },
      ),
      request<MemorySurfaceKnowledgeListResponse, { limit: number | null }>(
        "memory_surface/knowledge/list",
        { limit: 40 },
      ),
    ]);
    dispatch({ type: "about_you_set", surface: aboutYou.data.surface });
    dispatch({ type: "carrying_set", surface: carrying.data.surface });
    dispatch({ type: "knowledge_set", surface: knowledge.data.surface });
  }, [request]);

  const loadPeopleSurface = useCallback(async () => {
    const response = await request<PeopleSurfaceReadResponse, Record<string, never>>(
      "people_surface/read",
      {},
    );
    dispatch({ type: "people_set", surface: response.data.surface });
  }, [request]);

  const updateAboutYou = useCallback(
    async (update: AboutYouUpdate) => {
      const response = await request<MemorySurfaceAboutYouUpdateResponse, { update: AboutYouUpdate }>(
        "memory_surface/about_you/update",
        { update },
      );
      dispatch({ type: "about_you_set", surface: response.data.surface });
    },
    [request],
  );

  const correctKnowledge = useCallback(
    async (correction: KnowledgeCorrection) => {
      const response = await request<
        MemorySurfaceKnowledgeCorrectResponse,
        { correction: KnowledgeCorrection }
      >("memory_surface/knowledge/correct", { correction });
      dispatch({ type: "knowledge_item_upsert", item: response.data.item });
    },
    [request],
  );

  const upsertPersonBoundary = useCallback(
    async (person: PersonBoundary) => {
      const response = await request<PeopleSurfaceUpsertResponse, PeopleSurfaceUpsertRequest>(
        "people_surface/upsert",
        { person },
      );
      dispatch({ type: "people_set", surface: response.data.surface });
    },
    [request],
  );

  const deletePersonBoundary = useCallback(
    async (personId: string) => {
      const response = await request<PeopleSurfaceDeleteResponse, PeopleSurfaceDeleteRequest>(
        "people_surface/delete",
        { person_id: personId },
      );
      dispatch({ type: "people_set", surface: response.data.surface });
    },
    [request],
  );

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

  const attachWorkspaceMount = useCallback(
    async (mountRequest: WorkspaceMountAttachRequest) => {
      const response = await request<WorkspaceMountAttachResponse, WorkspaceMountAttachRequest>(
        "workspace_mount/attach",
        mountRequest,
      );
      dispatch({
        type: "workspace_mounts_set",
        mounts: [
          response.data.mount,
          ...state.workspaceMounts.filter(
            (mount) => mount.workspace_id !== response.data.mount.workspace_id,
          ),
        ],
      });
    },
    [request, state.workspaceMounts],
  );

  const detachWorkspaceMount = useCallback(
    async (workspaceId: WorkspaceMountId) => {
      const response = await request<WorkspaceMountDetachResponse, WorkspaceMountDetachRequest>(
        "workspace_mount/detach",
        { workspace_id: workspaceId },
      );
      dispatch({
        type: "workspace_mounts_set",
        mounts: state.workspaceMounts.filter(
          (mount) => mount.workspace_id !== response.data.workspace_id,
        ),
      });
    },
    [request, state.workspaceMounts],
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
      loadOwnerSurfaces,
      loadPeopleSurface,
      updateAboutYou,
      correctKnowledge,
      upsertPersonBoundary,
      deletePersonBoundary,
      updateRuntimeConfig,
      upsertProviderProfile,
      deleteProviderProfile,
      attachWorkspaceMount,
      detachWorkspaceMount,
    }),
    [
      attachWorkspaceMount,
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
      detachWorkspaceMount,
      listMemoryTopics,
      loadRuntimeState,
      loadOwnerSurfaces,
      loadPeopleSurface,
      memory,
      openMemoryEvidence,
      readMemoryWakeup,
      resumePanel,
      refreshThreads,
      respondToApproval,
      searchMemory,
      correctKnowledge,
      deletePersonBoundary,
      updateRuntimeConfig,
      updateAboutYou,
      upsertPersonBoundary,
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

const ownerInteractionContext = (browserInstanceId: string, threadId: ThreadId) => ({
  ingress: {
    kind: "web",
    source_id: browserInstanceId,
    conversation_id: `web:${browserInstanceId}:${threadId}`,
  },
  actor: {
    actor_id: "owner",
    kind: "owner" as const,
    display_name: "Owner",
    proof: "web-owner-session",
  },
  audience: { visibility: "private" as const, participants: ["owner"] },
  role: "private_companion" as const,
  authority: {
    can_speak_for_owner: false,
    can_start_work: true,
    can_call_tools: true,
    can_write_memory: true,
    requires_owner_confirmation: false,
  },
  disclosure: {
    allowed_topics: [],
    forbidden_topics: [],
    share_active_state: true,
    share_commitments: true,
    share_identity: true,
    evidence_policy: "expandable" as const,
  },
  task_mandate: null,
  provenance: {
    channel: { kind: "web" as const, external_conversation_id: `web:${browserInstanceId}:${threadId}` },
    received_at: null,
    authenticated_by: "web",
    raw_event_ref: null,
  },
});

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

const loadOwnerSurfacesWithClient = async (
  client: LizProtocolClient,
  dispatch: React.Dispatch<any>,
  setError: (error: string | null) => void,
) => {
  try {
    const [aboutYou, carrying, knowledge] = await Promise.all([
      client.request<MemorySurfaceAboutYouReadResponse, Record<string, never>>(
        "memory_surface/about_you/read",
        {},
      ),
      client.request<MemorySurfaceCarryingReadResponse, { limit: number | null }>(
        "memory_surface/carrying/read",
        { limit: 20 },
      ),
      client.request<MemorySurfaceKnowledgeListResponse, { limit: number | null }>(
        "memory_surface/knowledge/list",
        { limit: 40 },
      ),
    ]);
    if (aboutYou.ok) {
      dispatch({ type: "about_you_set", surface: aboutYou.data.surface });
    }
    if (carrying.ok) {
      dispatch({ type: "carrying_set", surface: carrying.data.surface });
    }
    if (knowledge.ok) {
      dispatch({ type: "knowledge_set", surface: knowledge.data.surface });
    }
  } catch (caught) {
    setError(messageFromError(caught));
  }
};

const messageFromError = (caught: unknown) => {
  const error = caught as Partial<ResponseError>;
  return error.message ?? "The Liz app server request failed.";
};
