# Privacy & Network Behavior

**oh-my-grok does not phone home to xAI/X.** It sends no analytics, usage
events, diagnostics, crash reports, traces, or product telemetry by default.
This document describes exactly what the binary does and does not send over the
network.

oh-my-grok is an independent, unofficial fork. The inherited telemetry that
upstream Grok Build ships is disabled by default here and cannot be re-enabled
by inherited build defaults, upstream config migration, or network-delivered
remote configuration — only by an explicit, local opt-in that you control.

## No telemetry by default

All of the following are **off** on a fresh install and stay off unless you
explicitly opt in on your own machine:

| Subsystem | Default | Notes |
| --- | --- | --- |
| Product analytics events | **Off** | No endpoint or API key is compiled in. The upstream `GROK_TELEMETRY_BUILD_*` compile-time baking has been removed, so a release build cannot ship an endpoint. |
| Mixpanel | **Off** | No project token is compiled in; the Mixpanel client is never constructed. |
| Internal OTLP trace firehose | **Off** | Only runs when telemetry is explicitly enabled *and* trace upload is on. |
| Session-artifact / GCS trace upload | **Off** | Same gate as above. |
| Error reporting (Sentry) | **Off** | No DSN is compiled in; inherits the (off) telemetry state. |
| Feedback submission | **Off** | Posts user-authored content to xAI infrastructure, so it is opt-in only. |
| External OpenTelemetry export | **Off** | Points at *your own* collector when you configure it — never xAI. |

Telemetry can be enabled only by an explicit local signal — the
`GROK_TELEMETRY_ENABLED` environment variable, the `[features] telemetry`
config key, or an administrator requirement pin. **Network-delivered remote
settings can never turn telemetry, trace upload, or feedback on.** If you enable
telemetry yourself, you must also supply your own endpoint/token; nothing is
sent to xAI/X unless you point it there.

## Crash reports stay local

If oh-my-grok crashes, a crash report is written to `~/.grok/crash/` on your
machine. It is **never uploaded automatically**. Nothing leaves your device
unless you choose to share the file yourself.

## Network traffic that is retained (and why)

The only network traffic oh-my-grok makes is for operations you explicitly
request:

| Destination | Purpose | When |
| --- | --- | --- |
| `cli-chat-proxy.grok.com`, `api.x.ai` | Model / inference API (chat completions) | When you send a prompt to an xAI/X model |
| `assets.grok.com` | Fetching assets required by a request | During a model request that needs them |
| `code.grok.com`, `grok.com/ws/gw` | Relay / cloud-sandbox WebSockets | Only with the web relay or `/cloud` |
| `auth.x.ai`, `accounts.x.ai` | X/xAI OAuth sign-in | When you log in |
| `.../deployment/config` | Managed deployment config | Only for enterprise deployments with a deployment key or team login |
| `github.com/estridell/oh-my-grok` | Update checks | The fork's own GitHub Releases — not xAI |

All model, asset, WebSocket, and OAuth endpoints above are the xAI/X provider
endpoints required to actually use xAI/X models; they are only contacted for
operations you initiate. Any of them can be repointed with the corresponding
`GROK_PRODUCTION_*` / endpoint override environment variables. Update checks go
to this fork's GitHub repository, not to xAI.

If you configure a different model provider (e.g. an OpenAI-compatible
endpoint), oh-my-grok talks to that provider instead, and no xAI/X traffic is
required at all.
