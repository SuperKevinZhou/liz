import type {
  ClientMethod,
  ClientRequestEnvelope,
  ClientTransportMessage,
  ConnectionState,
  RequestId,
  ResponseError,
  ServerEvent,
  ServerResponseEnvelope,
} from "./types";

type WebSocketFactory = (url: string) => WebSocketLike;

export interface WebSocketLike {
  readyState: number;
  send(data: string): void;
  close(): void;
  addEventListener(type: "open", listener: () => void): void;
  addEventListener(type: "close", listener: () => void): void;
  addEventListener(type: "error", listener: () => void): void;
  addEventListener(type: "message", listener: (event: { data: unknown }) => void): void;
}

export interface LizProtocolClientOptions {
  reconnectDelayMs?: number;
  requestTimeoutMs?: number;
  makeRequestId?: () => RequestId;
  webSocketFactory?: WebSocketFactory;
}

export interface LizProtocolClient {
  readonly state: ConnectionState;
  connect(url: string): void;
  close(): void;
  request<Data = unknown, Params = unknown>(
    method: ClientMethod,
    params: Params,
  ): Promise<ServerResponseEnvelope<Data>>;
  onState(listener: (state: ConnectionState) => void): () => void;
  onEvent(listener: (event: ServerEvent) => void): () => void;
  onUnknown(listener: (message: unknown) => void): () => void;
}

interface PendingRequest {
  resolve: (response: ServerResponseEnvelope) => void;
  reject: (error: ResponseError) => void;
  timer: ReturnType<typeof setTimeout>;
}

const defaultReconnectDelayMs = 900;
const defaultRequestTimeoutMs = 30_000;
const browserSocketFactory: WebSocketFactory = (url) => new WebSocket(url);

export class ProtocolClient implements LizProtocolClient {
  #state: ConnectionState = "idle";
  #url: string | null = null;
  #socket: WebSocketLike | null = null;
  #pending = new Map<RequestId, PendingRequest>();
  #stateListeners = new Set<(state: ConnectionState) => void>();
  #eventListeners = new Set<(event: ServerEvent) => void>();
  #unknownListeners = new Set<(message: unknown) => void>();
  #closedByClient = false;
  readonly #reconnectDelayMs: number;
  readonly #requestTimeoutMs: number;
  readonly #makeRequestId: () => RequestId;
  readonly #webSocketFactory: WebSocketFactory;

  constructor(options: LizProtocolClientOptions = {}) {
    this.#reconnectDelayMs = options.reconnectDelayMs ?? defaultReconnectDelayMs;
    this.#requestTimeoutMs = options.requestTimeoutMs ?? defaultRequestTimeoutMs;
    this.#makeRequestId = options.makeRequestId ?? defaultRequestId;
    this.#webSocketFactory = options.webSocketFactory ?? browserSocketFactory;
  }

  get state() {
    return this.#state;
  }

  connect(url: string) {
    this.#url = url;
    this.#closedByClient = false;
    this.#openSocket("connecting");
  }

  close() {
    this.#closedByClient = true;
    this.#socket?.close();
    this.#socket = null;
    this.#rejectPending({
      code: "client_closed",
      message: "The WebSocket connection closed before the request completed.",
      retryable: true,
    });
    this.#setState("closed");
  }

  request<Data = unknown, Params = unknown>(
    method: ClientMethod,
    params: Params,
  ): Promise<ServerResponseEnvelope<Data>> {
    if (!this.#socket || this.#socket.readyState !== WebSocket.OPEN) {
      return Promise.reject({
        code: "not_connected",
        message: "Connect to the Liz app server before sending requests.",
        retryable: true,
      } satisfies ResponseError);
    }

    const request_id = this.#makeRequestId();
    const envelope: ClientRequestEnvelope<Params> = { request_id, method, params };
    const frame: ClientTransportMessage<Params> = { kind: "request", payload: envelope };

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#pending.delete(request_id);
        reject({
          code: "request_timeout",
          message: "The Liz app server did not respond before the request timed out.",
          retryable: true,
        });
      }, this.#requestTimeoutMs);

      this.#pending.set(request_id, {
        resolve: resolve as (response: ServerResponseEnvelope) => void,
        reject,
        timer,
      });

      try {
        this.#socket?.send(JSON.stringify(frame));
      } catch {
        clearTimeout(timer);
        this.#pending.delete(request_id);
        reject({
          code: "send_failed",
          message: "The request could not be written to the WebSocket.",
          retryable: true,
        });
      }
    });
  }

  onState(listener: (state: ConnectionState) => void) {
    this.#stateListeners.add(listener);
    listener(this.#state);
    return () => this.#stateListeners.delete(listener);
  }

  onEvent(listener: (event: ServerEvent) => void) {
    this.#eventListeners.add(listener);
    return () => this.#eventListeners.delete(listener);
  }

  onUnknown(listener: (message: unknown) => void) {
    this.#unknownListeners.add(listener);
    return () => this.#unknownListeners.delete(listener);
  }

  #openSocket(state: ConnectionState) {
    if (!this.#url) {
      return;
    }

    this.#socket?.close();
    this.#setState(state);

    const socket = this.#webSocketFactory(this.#url);
    this.#socket = socket;
    socket.addEventListener("open", () => this.#setState("connected"));
    socket.addEventListener("message", (event) => this.#handleMessage(event.data));
    socket.addEventListener("error", () => {
      if (this.#state === "connected") {
        this.#setState("reconnecting");
      }
    });
    socket.addEventListener("close", () => this.#handleClose(socket));
  }

  #handleClose(socket: WebSocketLike) {
    if (socket !== this.#socket) {
      return;
    }

    this.#socket = null;
    this.#rejectPending({
      code: "connection_closed",
      message: "The Liz app server connection closed before the request completed.",
      retryable: true,
    });

    if (this.#closedByClient) {
      this.#setState("closed");
      return;
    }

    this.#setState("reconnecting");
    setTimeout(() => {
      if (!this.#closedByClient) {
        this.#openSocket("reconnecting");
      }
    }, this.#reconnectDelayMs);
  }

  #handleMessage(raw: unknown) {
    if (typeof raw !== "string") {
      this.#emitUnknown(raw);
      return;
    }

    let message: { kind?: string; payload?: unknown };
    try {
      message = JSON.parse(raw) as { kind?: string; payload?: unknown };
    } catch {
      this.#emitUnknown(raw);
      return;
    }

    if (message.kind === "response") {
      this.#resolveResponse(message.payload as ServerResponseEnvelope);
      return;
    }

    if (message.kind === "event") {
      this.#eventListeners.forEach((listener) => listener(message.payload as ServerEvent));
      return;
    }

    this.#emitUnknown(message);
  }

  #resolveResponse(response: ServerResponseEnvelope) {
    const pending = this.#pending.get(response.request_id);
    if (!pending) {
      this.#emitUnknown(response);
      return;
    }

    clearTimeout(pending.timer);
    this.#pending.delete(response.request_id);

    if (response.ok) {
      pending.resolve(response);
    } else {
      pending.resolve(response);
    }
  }

  #rejectPending(error: ResponseError) {
    this.#pending.forEach((pending) => {
      clearTimeout(pending.timer);
      pending.reject(error);
    });
    this.#pending.clear();
  }

  #emitUnknown(message: unknown) {
    this.#unknownListeners.forEach((listener) => listener(message));
  }

  #setState(state: ConnectionState) {
    if (state === this.#state) {
      return;
    }

    this.#state = state;
    this.#stateListeners.forEach((listener) => listener(state));
  }
}

const defaultRequestId = () => {
  if (globalThis.crypto?.randomUUID) {
    return globalThis.crypto.randomUUID();
  }

  return `request-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
};

export const createProtocolClient = (options?: LizProtocolClientOptions) =>
  new ProtocolClient(options);
