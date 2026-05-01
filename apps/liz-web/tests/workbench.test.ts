import {
  activeTranscript,
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
});

const event = (event_type: string, payload: unknown): ServerEvent => ({
  event_id: `event_${event_type}`,
  thread_id: "thread_01",
  turn_id: "turn_01",
  created_at: "2026-05-01T00:00:02Z",
  event_type,
  payload,
});
