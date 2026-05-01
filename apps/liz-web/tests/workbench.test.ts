import {
  activeTranscript,
  activeToolCalls,
  initialWorkbenchState,
  workbenchReducer,
} from "../src/state/workbench";
import type { ServerEvent, Thread, Turn } from "../src/protocol/types";

const thread: Thread = {
  id: "thread_01",
  title: "Console thread",
  status: "active",
  created_at: "2026-05-01T00:00:00Z",
  updated_at: "2026-05-01T00:00:00Z",
  active_goal: "Build web UI",
  active_summary: null,
  last_interruption: null,
  workspace_ref: null,
  pending_commitments: [],
  latest_turn_id: null,
  latest_checkpoint_id: null,
  parent_thread_id: null,
};

const turn: Turn = {
  id: "turn_01",
  thread_id: "thread_01",
  kind: "user",
  status: "running",
  started_at: "2026-05-01T00:00:01Z",
  ended_at: null,
  goal: "Build web UI",
  summary: null,
  checkpoint_before: null,
  checkpoint_after: null,
};

describe("workbench reducer", () => {
  it("projects assistant streaming into one transcript entry", () => {
    let state = workbenchReducer(initialWorkbenchState, {
      type: "threads_loaded",
      threads: [thread],
    });
    state = workbenchReducer(state, {
      type: "user_message_added",
      threadId: "thread_01",
      content: "Hello",
      createdAt: "2026-05-01T00:00:01Z",
    });
    state = workbenchReducer(state, { type: "turn_started", turn });
    state = workbenchReducer(state, {
      type: "server_event",
      event: event("assistant_chunk", { chunk: "Hel", stream_id: null, is_final: false }),
    });
    state = workbenchReducer(state, {
      type: "server_event",
      event: event("assistant_chunk", { chunk: "lo", stream_id: null, is_final: true }),
    });

    const transcript = activeTranscript(state);
    expect(transcript).toHaveLength(2);
    expect(transcript[1]).toMatchObject({
      kind: "assistant",
      content: "Hello",
      status: "completed",
    });
  });

  it("keeps thread updates sorted by freshness", () => {
    const staleThread = { ...thread, id: "thread_02", updated_at: "2026-04-30T00:00:00Z" };
    const freshThread = { ...thread, id: "thread_03", updated_at: "2026-05-02T00:00:00Z" };

    const state = workbenchReducer(initialWorkbenchState, {
      type: "threads_loaded",
      threads: [staleThread, freshThread],
    });

    expect(state.threads.map((item) => item.id)).toEqual(["thread_03", "thread_02"]);
  });

  it("groups tool lifecycle and executor output by call", () => {
    let state = workbenchReducer(initialWorkbenchState, {
      type: "threads_loaded",
      threads: [thread],
    });
    state = workbenchReducer(state, {
      type: "server_event",
      event: event("tool_call_started", {
        call_id: "call_01",
        tool_name: "shell",
        summary: "Run tests",
      }),
    });
    state = workbenchReducer(state, {
      type: "server_event",
      event: event("tool_call_committed", {
        call_id: "call_01",
        tool_name: "shell",
        arguments_summary: "npm test",
        risk_hint: "low",
      }),
    });
    state = workbenchReducer(state, {
      type: "server_event",
      event: event("executor_output_chunk", {
        executor_task_id: "exec_01",
        stream: "stdout",
        chunk: "passed",
      }),
    });
    state = workbenchReducer(state, {
      type: "server_event",
      event: event("tool_completed", {
        tool_name: "shell",
        summary: "Tests passed",
        artifact_ids: ["artifact_01"],
      }),
    });

    expect(activeToolCalls(state)[0]).toMatchObject({
      callId: "call_01",
      status: "completed",
      summary: "Tests passed",
      output: [{ chunk: "passed" }],
      artifactIds: ["artifact_01"],
    });
  });

  it("keeps approval requests after resolution", () => {
    let state = workbenchReducer(initialWorkbenchState, {
      type: "threads_loaded",
      threads: [thread],
    });
    state = workbenchReducer(state, {
      type: "server_event",
      event: event("approval_requested", {
        approval: {
          id: "approval_01",
          thread_id: "thread_01",
          turn_id: "turn_01",
          action_type: "shell",
          risk_level: "high",
          reason: "Run command",
          sandbox_context: "workspace-write",
          status: "pending",
        },
      }),
    });
    state = workbenchReducer(state, {
      type: "server_event",
      event: event("approval_resolved", {
        approval: {
          id: "approval_01",
          thread_id: "thread_01",
          turn_id: "turn_01",
          action_type: "shell",
          risk_level: "high",
          reason: "Run command",
          sandbox_context: "workspace-write",
          status: "approved",
        },
        decision: "approve_once",
      }),
    });

    expect(state.approvalsByThread.thread_01).toHaveLength(1);
    expect(state.approvalsByThread.thread_01[0].status).toBe("approved");
    expect(activeTranscript(state).filter((entry) => entry.kind === "system")).toHaveLength(2);
  });

  it("stores memory wakeup and compilation events", () => {
    let state = workbenchReducer(initialWorkbenchState, {
      type: "threads_loaded",
      threads: [thread],
    });
    state = workbenchReducer(state, {
      type: "server_event",
      event: event("memory_wakeup_loaded", {
        wakeup: {
          identity_summary: "Owner prefers concise updates.",
          active_state: "Building the web UI.",
          relevant_facts: ["Web console is active."],
          open_commitments: ["Ship the first UI."],
          recent_topics: ["liz-web"],
          recent_keywords: ["web"],
          citation_fact_ids: ["fact_01"],
          citations: [],
        },
      }),
    });
    state = workbenchReducer(state, {
      type: "server_event",
      event: event("memory_compilation_applied", {
        compilation: {
          delta_summary: "Updated active UI topic.",
          updated_fact_ids: ["fact_01"],
          invalidated_fact_ids: [],
          recent_topics: ["liz-web"],
          recent_keywords: ["console"],
          candidate_procedures: [],
        },
      }),
    });

    expect(state.memoryByThread.thread_01.wakeup?.identity_summary).toBe(
      "Owner prefers concise updates.",
    );
    expect(state.memoryByThread.thread_01.compilation?.delta_summary).toBe(
      "Updated active UI topic.",
    );
  });
});

const event = (event_type: string, payload: unknown): ServerEvent => ({
  event_id: `event_${event_type}`,
  thread_id: "thread_01",
  turn_id: "turn_01",
  created_at: "2026-05-01T00:00:02Z",
  event_type,
  payload,
});
