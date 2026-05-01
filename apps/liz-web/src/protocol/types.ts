export type RequestId = string;
export type ThreadId = string;
export type TurnId = string;
export type EventId = string;
export type ApprovalId = string;
export type ArtifactId = string;
export type MemoryFactId = string;

export type ConnectionState = "idle" | "connecting" | "connected" | "reconnecting" | "closed";

export interface Thread {
  id: ThreadId;
  title: string;
  status: ThreadStatus;
  created_at: string;
  updated_at: string;
  active_goal: string | null;
  active_summary: string | null;
  last_interruption: string | null;
  workspace_ref?: string | null;
  pending_commitments: string[];
  latest_turn_id: TurnId | null;
  latest_checkpoint_id: string | null;
  parent_thread_id: ThreadId | null;
}

export type ThreadStatus =
  | "active"
  | "waiting_approval"
  | "interrupted"
  | "completed"
  | "failed"
  | "archived";

export interface Turn {
  id: TurnId;
  thread_id: ThreadId;
  kind: "user" | "assistant" | "verification" | "compilation" | "rollback";
  status: "running" | "waiting_approval" | "cancelled" | "completed" | "failed";
  started_at: string;
  ended_at: string | null;
  goal: string | null;
  summary: string | null;
  checkpoint_before: string | null;
  checkpoint_after: string | null;
}

export interface ChannelRef {
  kind: "cli" | "telegram" | "discord" | "email" | "web" | "unknown";
  external_conversation_id: string;
}

export interface ParticipantRef {
  external_participant_id: string;
  display_name: string | null;
}

export interface ClientRequestEnvelope<Params = unknown> {
  request_id: RequestId;
  method: ClientMethod;
  params: Params;
}

export interface ClientTransportMessage<Params = unknown> {
  kind: "request";
  payload: ClientRequestEnvelope<Params>;
}

export type ClientMethod =
  | "provider_auth/list"
  | "model/status"
  | "runtime/config_get"
  | "runtime/config_update"
  | "provider_auth/upsert"
  | "provider_auth/delete"
  | "thread/start"
  | "thread/resume"
  | "thread/list"
  | "thread/fork"
  | "turn/start"
  | "turn/cancel"
  | "approval/respond"
  | "memory/read_wakeup"
  | "memory/compile_now"
  | "memory/list_topics"
  | "memory/search"
  | "memory/open_session"
  | "memory/open_evidence";

export interface ErrorResponseEnvelope {
  ok: false;
  request_id: RequestId;
  error: ResponseError;
}

export interface SuccessResponseEnvelope<Data = unknown> {
  ok: true;
  request_id: RequestId;
  method: string;
  data: Data;
}

export type ServerResponseEnvelope<Data = unknown> =
  | SuccessResponseEnvelope<Data>
  | ErrorResponseEnvelope;

export interface ResponseError {
  code: string;
  message: string;
  retryable: boolean;
}

export interface ServerTransportResponse<Data = unknown> {
  kind: "response";
  payload: ServerResponseEnvelope<Data>;
}

export interface ServerTransportEvent {
  kind: "event";
  payload: ServerEvent;
}

export interface UnknownServerTransportMessage {
  kind: string;
  payload?: unknown;
}

export type ServerTransportMessage =
  | ServerTransportResponse
  | ServerTransportEvent
  | UnknownServerTransportMessage;

export interface ServerEvent<Payload = unknown> {
  event_id: EventId;
  thread_id: ThreadId;
  turn_id: TurnId | null;
  created_at: string;
  event_type: ServerEventType | string;
  payload: Payload;
}

export type ServerEventType =
  | "thread_started"
  | "thread_resumed"
  | "thread_forked"
  | "thread_updated"
  | "thread_interrupted"
  | "thread_archived"
  | "turn_started"
  | "turn_completed"
  | "turn_failed"
  | "turn_cancelled"
  | "assistant_chunk"
  | "assistant_completed"
  | "tool_call_started"
  | "tool_call_updated"
  | "tool_call_committed"
  | "tool_completed"
  | "tool_failed"
  | "executor_output_chunk"
  | "approval_requested"
  | "approval_resolved"
  | "artifact_created"
  | "diff_available"
  | "checkpoint_created"
  | "memory_wakeup_loaded"
  | "memory_compilation_applied"
  | "memory_invalidation_applied"
  | "memory_dreaming_completed";

export interface ThreadStartRequest {
  title: string | null;
  initial_goal: string | null;
  workspace_ref: string | null;
}

export interface ThreadResumeRequest {
  thread_id: ThreadId;
}

export interface ThreadListRequest {
  status: ThreadStatus | null;
  limit: number | null;
}

export interface ThreadForkRequest {
  thread_id: ThreadId;
  title: string | null;
  fork_reason: string | null;
}

export interface TurnStartRequest {
  thread_id: ThreadId;
  input: string;
  input_kind: "user_message" | "steering_note" | "resume_command";
  channel?: ChannelRef;
  participant?: ParticipantRef;
}

export interface TurnCancelRequest {
  thread_id: ThreadId;
  turn_id: TurnId;
}

export interface ApprovalRespondRequest {
  approval_id: ApprovalId;
  decision: "approve_once" | "approve_and_persist" | "deny";
}

export interface ApprovalRequest {
  id: ApprovalId;
  thread_id: ThreadId;
  turn_id: TurnId;
  action_type: string;
  risk_level: "low" | "medium" | "high" | "critical";
  reason: string;
  sandbox_context: string | null;
  status: "pending" | "approved" | "denied" | "expired";
}

export interface MemoryReadWakeupRequest {
  thread_id: ThreadId;
}

export interface MemoryCompileNowRequest {
  thread_id: ThreadId;
}

export interface MemoryListTopicsRequest {
  status: "active" | "resolved" | "stale" | null;
  limit: number | null;
}

export interface MemorySearchRequest {
  query: string;
  mode: "keyword" | "semantic";
  limit: number | null;
}

export interface MemoryOpenEvidenceRequest {
  thread_id: ThreadId;
  turn_id: TurnId | null;
  artifact_id: ArtifactId | null;
  fact_id: MemoryFactId | null;
}

export interface MemoryOpenSessionRequest {
  thread_id: ThreadId;
}

export interface RuntimeConfigUpdateRequest {
  sandbox: unknown | null;
  approval_policy: "on-request" | "danger-full-access" | null;
}

export interface ProviderAuthListRequest {
  provider_id: string | null;
}

export interface ProviderAuthProfile {
  profile_id: string;
  provider_id: string;
  display_name: string | null;
  credential: ProviderCredential;
}

export type ProviderCredential =
  | { kind: "api_key"; api_key: string }
  | {
      kind: "oauth";
      access_token: string;
      refresh_token: string | null;
      expires_at_ms: number | null;
      account_id: string | null;
      email: string | null;
    }
  | {
      kind: "token";
      token: string;
      expires_at_ms: number | null;
      metadata: Record<string, string>;
    };

export interface ProviderAuthUpsertRequest {
  profile: ProviderAuthProfile;
}

export interface ProviderAuthDeleteRequest {
  profile_id: string;
}

export interface ThreadStartResponse {
  thread: Thread;
}

export interface ThreadResumeResponse {
  thread: Thread;
  resume_summary: ResumeSummary | null;
}

export interface ThreadListResponse {
  threads: Thread[];
}

export interface ThreadForkResponse {
  thread: Thread;
}

export interface TurnStartResponse {
  turn: Turn;
}

export interface TurnCancelResponse {
  turn: Turn;
}

export interface ApprovalRespondResponse {
  approval: ApprovalRequest;
}

export interface MemoryCitationRef {
  thread_id: ThreadId;
  turn_id: TurnId | null;
  artifact_id: ArtifactId | null;
  note: string;
}

export interface MemoryWakeup {
  identity_summary: string | null;
  active_state: string | null;
  relevant_facts: string[];
  open_commitments: string[];
  recent_topics: string[];
  recent_keywords: string[];
  citation_fact_ids: MemoryFactId[];
  citations: MemoryCitationRef[];
}

export interface RecentConversationWakeupView {
  recent_summaries: string[];
  active_topics: string[];
  recent_keywords: string[];
  citations: MemoryCitationRef[];
}

export interface MemoryCompilationSummary {
  delta_summary: string;
  updated_fact_ids: MemoryFactId[];
  invalidated_fact_ids: MemoryFactId[];
  recent_topics: string[];
  recent_keywords: string[];
  candidate_procedures: string[];
}

export interface MemoryTopicSummary {
  name: string;
  aliases: string[];
  summary: string;
  status: "active" | "resolved" | "stale";
  last_active_at: string | null;
  related_thread_ids: ThreadId[];
  related_artifact_ids: ArtifactId[];
  citation_fact_ids: MemoryFactId[];
  recent_keywords: string[];
}

export interface MemorySearchHit {
  kind: "topic" | "session" | "fact" | "artifact";
  title: string;
  summary: string;
  score: number;
  thread_id: ThreadId | null;
  turn_id: TurnId | null;
  artifact_id: ArtifactId | null;
  fact_id: MemoryFactId | null;
}

export interface ArtifactRef {
  id: ArtifactId;
  thread_id: ThreadId;
  turn_id: TurnId;
  kind: string;
  summary: string;
  locator: string;
  created_at: string;
}

export interface MemoryEvidenceView {
  citation: MemoryCitationRef;
  thread_title: string | null;
  turn_summary: string | null;
  fact_id: MemoryFactId | null;
  fact_kind: string | null;
  fact_value: string | null;
  artifact: ArtifactRef | null;
  artifact_body: string | null;
}

export interface MemorySessionEntry {
  recorded_at: string;
  event: string;
  summary: string;
  turn_id: TurnId | null;
  artifact_ids: ArtifactId[];
}

export interface MemorySessionView {
  thread_id: ThreadId;
  title: string;
  status: ThreadStatus;
  active_summary: string | null;
  pending_commitments: string[];
  recent_entries: MemorySessionEntry[];
  artifacts: ArtifactRef[];
}

export interface MemoryReadWakeupResponse {
  thread_id: ThreadId;
  wakeup: MemoryWakeup;
  recent_conversation: RecentConversationWakeupView;
}

export interface MemoryCompileNowResponse {
  thread_id: ThreadId;
  compilation: MemoryCompilationSummary;
}

export interface MemoryListTopicsResponse {
  topics: MemoryTopicSummary[];
}

export interface MemorySearchResponse {
  query: string;
  mode: "keyword" | "semantic";
  hits: MemorySearchHit[];
}

export interface MemoryOpenEvidenceResponse {
  evidence: MemoryEvidenceView;
}

export interface MemoryOpenSessionResponse {
  session: MemorySessionView;
}

export interface RuntimeConfigResponse {
  sandbox: {
    mode: string;
    network: string;
    working_directory: string | null;
  };
  approval_policy: "on-request" | "danger-full-access";
}

export interface ProviderAuthListResponse {
  profiles: ProviderAuthProfile[];
}

export interface ProviderAuthUpsertResponse {
  profile: ProviderAuthProfile;
}

export interface ProviderAuthDeleteResponse {
  profile_id: string;
}

export interface ModelStatusResponse {
  provider_id: string;
  display_name: string | null;
  model_id: string | null;
  auth_kind: string | null;
  ready: boolean;
  credential_configured: boolean;
  credential_hints: string[];
  notes: string[];
}

export interface ResumeSummary {
  headline: string;
  active_summary: string | null;
  pending_commitments: string[];
  last_interruption: string | null;
}

export interface ThreadEventPayload {
  thread: Thread;
}

export interface TurnEventPayload {
  turn: Turn;
}

export interface TurnFailedEventPayload {
  turn: Turn;
  message: string;
}

export interface AssistantChunkEventPayload {
  chunk: string;
  stream_id: string | null;
  is_final: boolean;
}

export interface AssistantCompletedEventPayload {
  message: string;
}

export interface ToolCallStartedEventPayload {
  call_id: string;
  tool_name: string;
  summary: string;
}

export interface ToolCallUpdatedEventPayload {
  call_id: string;
  tool_name: string;
  delta_summary: string;
  preview: string | null;
}

export interface ToolCallCommittedEventPayload {
  call_id: string;
  tool_name: string;
  arguments_summary: string;
  risk_hint: "low" | "medium" | "high" | "critical" | null;
}

export interface ToolCompletedEventPayload {
  tool_name: string;
  summary: string;
  artifact_ids: ArtifactId[];
}

export interface ToolFailedEventPayload {
  tool_name: string;
  summary: string;
}

export interface ExecutorOutputChunkEventPayload {
  executor_task_id: string;
  stream: "stdout" | "stderr";
  chunk: string;
}

export interface ApprovalRequestedEventPayload {
  approval: ApprovalRequest;
}

export interface ApprovalResolvedEventPayload {
  approval: ApprovalRequest;
  decision: "approve_once" | "approve_and_persist" | "deny";
}
