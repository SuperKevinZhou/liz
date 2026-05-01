import { createProtocolClient, type WebSocketLike } from "../src/protocol/client";

class MockWebSocket implements WebSocketLike {
  static readonly sockets: MockWebSocket[] = [];
  readyState: number = WebSocket.CONNECTING;
  sent: string[] = [];
  listeners = new Map<string, Array<(event?: any) => void>>();

  constructor(readonly url: string) {
    MockWebSocket.sockets.push(this);
  }

  send(data: string) {
    this.sent.push(data);
  }

  close() {
    this.readyState = WebSocket.CLOSED;
    this.emit("close");
  }

  addEventListener(type: "open" | "close" | "error" | "message", listener: (event?: any) => void) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  open() {
    this.readyState = WebSocket.OPEN;
    this.emit("open");
  }

  receive(data: unknown) {
    this.emit("message", { data });
  }

  private emit(type: string, event?: any) {
    this.listeners.get(type)?.forEach((listener) => listener(event));
  }
}

beforeEach(() => {
  MockWebSocket.sockets.length = 0;
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("ProtocolClient", () => {
  it("correlates request and response frames", async () => {
    const client = createProtocolClient({
      makeRequestId: () => "request_01",
      webSocketFactory: (url) => new MockWebSocket(url),
    });
    client.connect("ws://127.0.0.1:7777");
    MockWebSocket.sockets[0].open();

    const responsePromise = client.request("thread/list", { status: null, limit: 50 });
    const frame = JSON.parse(MockWebSocket.sockets[0].sent[0]);

    expect(frame).toEqual({
      kind: "request",
      payload: {
        request_id: "request_01",
        method: "thread/list",
        params: { status: null, limit: 50 },
      },
    });

    MockWebSocket.sockets[0].receive(
      JSON.stringify({
        kind: "response",
        payload: {
          ok: true,
          request_id: "request_01",
          method: "thread/list",
          data: { threads: [] },
        },
      }),
    );

    await expect(responsePromise).resolves.toMatchObject({
      ok: true,
      data: { threads: [] },
    });
  });

  it("dispatches events separately from responses", () => {
    const client = createProtocolClient({
      webSocketFactory: (url) => new MockWebSocket(url),
    });
    const events: unknown[] = [];
    client.onEvent((event) => events.push(event));
    client.connect("ws://127.0.0.1:7777");
    MockWebSocket.sockets[0].open();

    MockWebSocket.sockets[0].receive(
      JSON.stringify({
        kind: "event",
        payload: {
          event_id: "event_01",
          thread_id: "thread_01",
          turn_id: null,
          created_at: "2026-05-01T00:00:00Z",
          event_type: "assistant_chunk",
          payload: { chunk: "hello", stream_id: null, is_final: false },
        },
      }),
    );

    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({
      event_type: "assistant_chunk",
      payload: { chunk: "hello" },
    });
  });

  it("clears pending requests when the socket reconnects", async () => {
    const client = createProtocolClient({
      makeRequestId: () => "request_02",
      webSocketFactory: (url) => new MockWebSocket(url),
      reconnectDelayMs: 25,
    });
    const states: string[] = [];
    client.onState((state) => states.push(state));
    client.connect("ws://127.0.0.1:7777");
    MockWebSocket.sockets[0].open();

    const responsePromise = client.request("thread/list", { status: null, limit: 50 });
    MockWebSocket.sockets[0].close();
    await expect(responsePromise).rejects.toMatchObject({ code: "connection_closed" });

    await vi.advanceTimersByTimeAsync(25);
    expect(MockWebSocket.sockets).toHaveLength(2);
    expect(states).toContain("reconnecting");
  });

  it("emits unknown frames without throwing", () => {
    const client = createProtocolClient({
      webSocketFactory: (url) => new MockWebSocket(url),
    });
    const unknown: unknown[] = [];
    client.onUnknown((message) => unknown.push(message));
    client.connect("ws://127.0.0.1:7777");
    MockWebSocket.sockets[0].open();

    MockWebSocket.sockets[0].receive("{broken json");
    MockWebSocket.sockets[0].receive(JSON.stringify({ kind: "future_frame", payload: {} }));

    expect(unknown).toHaveLength(2);
  });
});
