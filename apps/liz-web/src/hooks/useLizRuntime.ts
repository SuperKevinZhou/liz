import { useCallback, useMemo, useReducer, useRef, useState } from "react";
import { createProtocolClient, type LizProtocolClient } from "../protocol/client";
import type {
  ApprovalRespondRequest,
  ConnectionState,
  ResponseError,
  ServerResponseEnvelope,
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
  TurnId,
  TurnStartRequest,
  TurnStartResponse,
} from "../protocol/types";
import {
  activeRuntime,
  activeThread,
  activeTranscript,
  initialWorkbenchState,
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
  connect: () => void;
  close: () => void;
  refreshThreads: () => Promise<void>;
  setActiveThread: (threadId: ThreadId) => Promise<void>;
  startThread: (request: ThreadStartRequest) => Promise<void>;
  forkThread: (request: ThreadForkRequest) => Promise<void>;
  startTurn: (input: string) => Promise<void>;
  cancelTurn: () => Promise<void>;
  respondToApproval: (request: ApprovalRespondRequest) => Promise<void>;
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
            type: "resume_summary_added",
            threadId,
            content: [summary.headline, summary.active_summary].filter(Boolean).join("\n"),
            createdAt: new Date().toISOString(),
          });
        }
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
      await request("approval/respond", approvalRequest);
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
      connect,
      close,
      refreshThreads,
      setActiveThread,
      startThread,
      forkThread,
      startTurn,
      cancelTurn,
      respondToApproval,
    }),
    [
      cancelTurn,
      close,
      connect,
      connectionState,
      currentThread,
      error,
      forkThread,
      refreshThreads,
      respondToApproval,
      runtime,
      setActiveThread,
      startThread,
      startTurn,
      state,
      transcript,
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

const messageFromError = (caught: unknown) => {
  const error = caught as Partial<ResponseError>;
  return error.message ?? "The Liz app server request failed.";
};
