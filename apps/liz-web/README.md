# Liz Web Console

`liz-web` is the browser console for the Liz app server. It runs as an independent Vite application and connects to the existing app-server WebSocket transport.

## Development

Start the app server first:

```powershell
cargo run -p liz-app-server
```

The server prints its WebSocket URL on startup. The web console defaults to:

```text
ws://127.0.0.1:8787
```

If the server is listening on another port, update the Server URL field in the top bar. The value is stored in browser local storage.

Install and start the web console:

```powershell
cd apps/liz-web
npm install
npm run dev
```

Vite serves the app on the first available local port starting at `5174`.

## Verification

Run unit tests:

```powershell
npm test
```

Build the production bundle:

```powershell
npm run build
```

Run Playwright smoke tests after the browser runtime is installed:

```powershell
npx playwright install chromium
npm run e2e
```

## Runtime Surface

The console sends `turn/start` requests with:

- `channel.kind = "web"`
- `channel.external_conversation_id = "web:<browser_instance_id>:<thread_id>"`
- `participant.external_participant_id = "owner"`
- `participant.display_name = "Owner"`

Conversation-only threads leave `workspace_ref` empty. Workspace-attached threads pass the workspace path through `thread/start` and display it in the thread panel and composer.

Provider credential values are never rendered back into the UI after storage. The client masks any credential payload returned by the server before putting provider profiles into UI state.
