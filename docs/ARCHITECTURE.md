# Architecture

One crate, split into model, viewmodel and view layers.

```
src/
  main.rs            CLI args, calls app::run
  app.rs             builds the concrete types, routes intents, runs eframe
  domain/            data and policy
  services/          file, process and network helpers
  proxy/             the MITM proxy
  viewmodels/        UI state and intent handlers
  views/             egui drawing
```

## Layers

| Layer | Holds | Depends on |
|---|---|---|
| `domain`, `services`, `proxy` | data, policy, I/O | std and crates |
| `viewmodels` | UI state, intents, derived data (visible rows, tree, diff) | the layer above |
| `views` | egui widgets | viewmodels |
| `app.rs` | wiring | everything |

- `domain/`, `services/`, `proxy/` and `viewmodels/` don't import `egui` or `eframe`.
- Repaint requests go through the `domain::repaint::Repaint` trait. `app.rs` implements it for `egui::Context`.
- A view reads a viewmodel and returns intents. Text fields may edit a viewmodel field directly; everything else is an intent.

## Data flow

1. The proxy sends `Event`s to a `FlowSink` (`proxy/sink.rs`). `ChannelSink` puts them on an `mpsc` channel and requests a repaint.
2. Each frame, `App::drain` calls `FlowStore::pump`. It applies events to the store, enforces the 20k flow cap, and passes `Paused` and `Log` events to the rules viewmodel.
3. Viewmodels read the shared `FlowStore` and `Config`.
4. Views return intents (`viewmodels/intent.rs`).
5. `app.rs` hands each intent to a viewmodel's `handle_*`. If another viewmodel needs to react (say, open a replay window), the handler returns an `Effect` and `App::apply` routes it.

## Shared state

- `FlowStore` (`services/flow_store.rs`) holds all captured flows behind `Arc<Mutex<Inner>>`. `Inner::rev()` goes up on every change; viewmodels use it as a cache key. The REST API reads the same store.
- `Shared` (`domain/config.rs`) is `Arc<Mutex<Config>>`, read when needed, so rule edits apply to the next request. `should_decrypt` is only checked on CONNECT, so "don't decrypt" applies to new connections only.
- Mute, scope and exclude only affect what is recorded (`Config::captured`), not the network. Scope and exclude entries are hosts or `host/path*` URL patterns; CONNECT tunnels are never judged, only the requests inside them.

Keep lock guards short. Don't hold the store lock across an `.await`, and don't call the store from the closure passed to `FlowStore::pump`.

## Files

### `domain/`
| File | Contents |
|---|---|
| `flow.rs` | `Flow`, `Message`, `WsMsg`, `Event`, breakpoint types |
| `rule.rs` | `Rule` and `Action` (data, label, description) |
| `config.rs` | `Config`, `Upstream`, `UpstreamMode` (HTTP, SOCKS5 or PAC), `Shared` |
| `policy.rs` | `Config::captured` (scope, exclude, mute against the full URL), `should_decrypt`, `matching`; URL and glob matching |
| `filter.rs` | search query parser |
| `mime.rs` | file extension to content type |
| `repaint.rs` | `Repaint` trait |

### `services/`
| File | Contents |
|---|---|
| `flow_store.rs` | `FlowStore`, event handling (`apply`), flow cap, `pump` |
| `config_store.rs` | read and write `config.json` |
| `session.rs` | `.pxp` save and load, autosave path |
| `importer.rs`, `exporter.rs` | HAR and Charles JSON in; HAR, CSV, curl, fetch, python, httpie, raw out |
| `decode.rs` | pretty-print, JWT, form, protobuf without a schema, diff |
| `decoders/` | `BodyDecoder` trait and one file per body view |
| `schemas.rs` | protobuf descriptors |
| `graphql.rs` | GraphQL reports |
| `replay.rs` | send a request without going through the proxy |
| `devices.rs` | system proxy, CA trust, Chrome, iOS simulator, adb |
| `api.rs` | loopback API on port + 1: read-only REST, plus `POST /mcp`; rejects non-loopback `Host`, foreign `Origin` and non-JSON posts |
| `mcp.rs` | MCP JSON-RPC resources/tools over HTTP and stdio |

### `proxy/`
| File | Contents |
|---|---|
| `mod.rs` | `run` builds the hudsucker proxy |
| `handler.rs` | `HttpHandler` and `WebSocketHandler`; calls the pipeline |
| `actions/` | `RuleAction` trait and one file per rule type. `resolve` maps `Action` data to an implementation |
| `pipeline.rs` | runs a list of `RuleAction`s: `plan`, `short_circuit`, `apply_request_actions`, `apply_response_actions` |
| `http_util.rs` | header, body and URI helpers |
| `sink.rs` | `FlowSink`, `ChannelSink` |
| `tls.rs` | CA load, create and import; key logger |
| `connector/` | DNS overrides, upstream proxy (manual or PAC) |
| `script.rs` | JavaScript rule hooks |
| `app_lookup.rs` | finds the client process with `lsof` |

### `viewmodels/`
| File | Holds |
|---|---|
| `capture_vm.rs` | filter, sort, selection, visible rows, sidebar tree; row, sidebar and shortcut intents |
| `detail_vm.rs` | tabs, view modes, diff base and cache |
| `rules_vm.rs` | rule form, breakpoints, console, scope lists, `RuleKind` |
| `replay_vm.rs` | replay and compose drafts |
| `network_vm.rs`, `schemas_vm.rs` | the Network and Schemas tabs of Settings |
| `settings_vm.rs` | Settings window visibility and tab |
| `session_vm.rs` | autosave, session and rules files |
| `devices_vm.rs` | device actions; simulator and system-proxy state polled off the UI thread |
| `onboarding_vm.rs` | onboarding stages; device and import intents become `Effect`s |
| `status.rs` | status line, `JobRunner` for background jobs |
| `intent.rs` | intent and effect enums |

### `views/`
One file per screen area: `toolbar`, `sidebar`, `table`, `detail`, `rules_panel`, `breakpoint_window`, `replay_window`, `settings_window` (with `network_tab` and `schemas_tab`), `shortcuts`, `body`. Also `theme` and `icon`.

## Extending

- A rule type implements `proxy::actions::RuleAction`. The pipeline only knows the trait.
- A body view implements `services::decoders::BodyDecoder` and returns a `Rendered` value (text, form, tree, hex or sections). Views draw `Rendered` and never match on a decoder.

Neither should panic. They return values or change the message they are given.
