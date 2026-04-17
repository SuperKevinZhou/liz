# Provider Support Matrix

This document tracks the current provider runtime status for `liz`.

## Runtime policy

- `liz` now defaults to live provider execution.
- Test-only simulated streaming must be injected explicitly through `ModelGateway::simulated()` or `AppServer::new_simulated(...)`.
- Providers marked as `Unimplemented` fail fast when selected in live runtime.
- Generic OpenAI-compatible support means `liz` can connect through the shared chat-completions path, but does not claim provider-native model discovery, native auth UX, or custom transport behavior beyond that compatibility surface.

## Status legend

| Status | Meaning |
| --- | --- |
| `Ready (native)` | Live runtime path is implemented with provider-family or provider-specific routing/auth handling. |
| `Ready (generic)` | Live runtime path works through the generic OpenAI-compatible adapter with a known default or derived base URL. |
| `Ready (conditional)` | Live runtime path is implemented, but depends on local services or additional routing metadata such as account/project env vars. |
| `Unimplemented` | Provider remains listed for planning/reference, but live runtime selection fails fast. |

## Matrix

| Provider(s) | Status | Integration | Auth | Key functionality | Notes |
| --- | --- | --- | --- | --- | --- |
| `openai` | `Ready (native)` | OpenAI Responses adapter | API key | Streaming, prompt caching, live Responses path | Primary OpenAI path |
| `openai-codex` | `Ready (native)` | Native Codex Responses route | OAuth | Refresh token handling, server-side conversation state, prompt caching | Uses ChatGPT/Codex OAuth and `/codex/responses` |
| `anthropic` | `Ready (native)` | Anthropic Messages adapter | API key | Messages API, image input, reasoning-oriented family support | Uses provider beta headers where configured |
| `google` | `Ready (native)` | Google Generative AI adapter | API key | `generateContent` routing | Gemini direct API |
| `google-vertex` | `Ready (native)` | Vertex `generateContent` adapter | Google ADC / bearer | Vertex project/location routing | Requires Google project metadata or ADC |
| `google-vertex-anthropic`, `anthropic-vertex` | `Ready (native)` | Vertex Anthropic raw predict adapter | Google ADC / bearer | Anthropic-on-Vertex routing | Uses Anthropic-compatible prompt shape over Vertex |
| `amazon-bedrock` | `Ready (native)` | Bedrock Converse adapter | AWS credential chain or bearer | Converse request signing, model/region routing | Native Bedrock path |
| `amazon-bedrock-mantle` | `Ready (native)` | OpenAI-style adapter with Bedrock Mantle auth | AWS credential chain or bearer | OpenAI-compatible request shape | Uses Mantle bearer resolution |
| `github-copilot` | `Ready (native)` | Provider-owned GitHub Copilot routing | Device code / token exchange | Runtime token exchange, provider-owned mode selection | Supports chat/responses/messages depending on model |
| `gitlab` | `Ready (native)` | Provider-owned GitLab Duo routing | OAuth or PAT | GitLab auth handling, Duo chat endpoint | Hybrid auth |
| `xai` | `Ready (native)` | OpenAI Responses adapter | API key | Responses-style path | xAI-specific default route |
| `opencode` | `Ready (native)` | OpenAI Responses adapter | API key | Zen Responses path | Uses OpenCode Zen defaults |
| `opencode-go` | `Ready (native)` | OpenAI-compatible adapter | API key | Chat completions path | Uses OpenCode Go defaults |
| `qwen` | `Ready (native)` | OpenAI-compatible adapter with endpoint normalization | API key | Version-aware chat completions path | Supports multiple Qwen endpoint families |
| `zai` | `Ready (native)` | OpenAI-compatible adapter with endpoint normalization | API key | Version-aware chat completions path | Supports coding/general Z.AI routes |
| `cloudflare-ai-gateway` | `Ready (native)` | OpenAI-compatible adapter with derived gateway route | API key | Gateway routing | Can derive base URL from account and gateway env vars |
| `sap-ai-core` | `Ready (native)` | OpenAI-compatible adapter with service-key auth | Service key | OAuth token minting, deployment-scoped route | Provider-specific auth flow |
| `minimax` | `Ready (native)` | Anthropic Messages adapter | API key | Anthropic-compatible routing | Supports global/CN API lanes |
| `minimax-portal` | `Ready (native)` | Anthropic Messages adapter | OAuth | OAuth refresh and resource URL routing | Portal-specific auth handling |
| `kimi` | `Ready (native)` | Anthropic Messages adapter | API key | Kimi coding endpoint | Dedicated Kimi coding route |
| `azure` | `Ready (native)` | OpenAI-compatible adapter with Azure routing | API key | Deployment-aware chat completions | Derives base URL from Azure resource name |
| `azure-cognitive-services` | `Ready (native)` | OpenAI-compatible adapter with Azure Cognitive routing | API key | Deployment-aware chat completions | Derives Cognitive Services base URL |
| `microsoft-foundry` | `Ready (native)` | OpenAI-compatible adapter with Foundry routing | API key | Deployment-aware chat completions | Derives Foundry base URL |
| `byteplus`, `byteplus-plan` | `Ready (native)` | OpenAI-compatible adapter | API key | Standard and coding-plan routing | Separate default bases for standard vs coding |
| `stepfun`, `stepfun-plan` | `Ready (native)` | OpenAI-compatible adapter | API key | Standard and coding-plan routing | Separate default bases for standard vs coding |
| `volcengine`, `volcengine-plan` | `Ready (native)` | OpenAI-compatible adapter | API key | Standard and coding-plan routing | Separate default bases for standard vs coding |
| `openrouter` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions, tool-call capable path | Default base URL is built in |
| `deepseek` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Compatibility alias with known base URL |
| `mistral` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Compatibility alias with known base URL |
| `moonshot`, `moonshotai` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Compatibility alias with known base URL |
| `together`, `togetherai` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Compatibility alias with known base URL |
| `302ai` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Compatibility alias with known base URL |
| `cohere` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Uses Cohere compatibility route |
| `cortecs` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Compatibility alias with known base URL |
| `groq` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Uses Groq OpenAI-compatible route |
| `helicone` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Compatibility alias with known base URL |
| `io-net` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Compatibility alias with known base URL |
| `nebius`, `nebius-token-factory` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Compatibility alias with known base URL |
| `ollama-cloud` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Hosted Ollama route |
| `ovhcloud`, `ovhcloud-ai-endpoints` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Compatibility alias with known base URL |
| `scaleway` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Compatibility alias with known base URL |
| `stackit` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Compatibility alias with known base URL |
| `vercel` | `Ready (generic)` | Generic OpenAI-compatible | API key | Chat completions | Vercel AI Gateway compatibility route |
| `ollama`, `llama.cpp`, `lmstudio`, `vllm`, `sglang` | `Ready (conditional)` | Generic OpenAI-compatible | Local | Chat completions | Requires a local or self-hosted runtime at the configured/default port |
| `cloudflare-workers-ai` | `Ready (conditional)` | Generic OpenAI-compatible | API key | Chat completions | Requires `CLOUDFLARE_ACCOUNT_ID` to derive the runtime base URL |
| `copilot-proxy` | `Ready (conditional)` | OpenAI-compatible local proxy | Local | Chat completions | Assumes a local proxy endpoint |
| `arcee`, `baseten`, `cerebras`, `chutes`, `deepinfra`, `fireworks`, `fireworks-ai`, `firmware`, `huggingface`, `kilo`, `kilocode`, `litellm`, `nvidia`, `poe`, `qianfan`, `synthetic`, `venice`, `xiaomi`, `zenmux` | `Unimplemented` | Reserved compatibility aliases | Varies | None in live runtime | Selecting these providers fails fast until a verified live endpoint or provider-specific routing metadata is added |

## Reserved adapter surfaces

| Surface | Status | Notes |
| --- | --- | --- |
| `openai-compatible` adapter surface | Reserved | Not used as a standalone selectable provider; current live support is routed through `OpenAiStyleAdapter` |
| `local-gateway` adapter surface | Reserved | Placeholder for a future local gateway integration |
