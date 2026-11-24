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
- A view reads a viewmodel and returns intents. A widget may bind a viewmodel field directly only when the field is pure view state: text being typed, the selected tab or view mode, whether a window or panel is open, and one-shot focus requests the view consumes. Anything that changes captured flows, config, or another viewmodel is an intent.

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

- Proxy-only hosts (`Config::proxy_only_hosts`) are network level, unlike scope: with the system proxy on, `devices::apply_route` writes `proxyproxy.pac` in the app data dir and sets it as the macOS auto-proxy URL, so other hosts connect directly. `DevicesVm::tick` rewrites the file when the list or toggle changes while the proxy is on. Only hosts are expressible, so path patterns are reduced to their host. Connections already open keep their old route, and WebKit sandboxed apps and some Windows apps may ignore `file://` PACs.

- Platform support: the system proxy and PAC live in `services/sysproxy/`, one backend per OS: macOS `networksetup`, Linux GNOME `gsettings` (mode `manual` or `auto` plus `autoconfig-url`), Windows `reg` on `HKCU\...\Internet Settings` (`ProxyEnable`, `ProxyServer`, `AutoConfigURL`). Other desktops, or a missing tool, give an error hint in the status line and the setting reads as off. PAC URLs come from `domain::pac::file_url` (`file:///C:/...` on Windows). The client process comes from `services/client_proc.rs`: `/proc/net/tcp` plus `/proc/*/fd` on Linux, `netstat -ano` plus `tasklist` on Windows, `lsof` elsewhere. Linux apps get a name and icon from their `.desktop` entry and the hicolor theme; Windows apps get the executable name and a letter avatar. iOS Simulator, `.app` bundles and `Info.plist` (`plutil`) are macOS only. Output parsers are in `services/os_parse.rs` and compile everywhere, so their fixtures run on any host. Whole-crate `cargo check` for Linux and Windows needs a C toolchain for those targets (aws-lc), so only the platform modules were cross-checked.

- App identity is lazy: the flow gets its process name first (`Event::App` with no identity), then the bundle name, id and icon key once `app_icons::resolve` finishes. Lookup failures leave the name alone and never delay a request. Icons come from `.icns` (macOS), the app's `Assets.car` (`services::car`, pure Rust, so iOS Simulator apps get their real icon), or the largest loose `AppIcon*.png`; failing all three the view draws a letter avatar (bundled apps) or a terminal glyph (bare processes like curl). `AppIdentity::platform` is set for iOS Simulator and Android apps; the sidebar nests those under one phone-icon folder per platform, apart from host apps. Android emulators and `adb` USB devices are named from a package id (`domain::app::package_hint`: `X-Requested-With` first, since WebViews send the package there, then a `com.foo.bar/1.2` token in the User-Agent). `services::android_icons` then asks `adb` (`ANDROID_HOME`/`ANDROID_SDK_ROOT` platform-tools, else `PATH`) for `pm path`, pulls the base APK, and reads label and icon in Rust (`zip_read`, `android_pkg` for binary XML and `resources.arsc`), with the `Adb` trait seam faked in tests. Limits: okhttp and plain HTTP clients send neither header, so those flows stay "Android Emulator"; adaptive icons are XML, so the foreground PNG is used when the APK ships one; no adb, an offline device or an unknown package leaves the package id as the name. Results are cached by package and remote APK path (it changes on update) under `<data dir>/icons/`.

Keep lock guards short. Don't hold the store lock across an `.await`, and don't call the store from the closure passed to `FlowStore::pump`.

## Files

### `domain/`
| File | Contents |
|---|---|
| `flow.rs` | `Flow`, `Message`, `WsMsg`, `Event`, breakpoint types |
| `rule.rs` | `Rule` and `Action` (data, label, description) |
| `config.rs` | `Config`, `Upstream`, `UpstreamMode` (HTTP, SOCKS5 or PAC), `Shared` |
| `pac.rs` | `Route`, `Config::proxy_route`, proxy-only host normalizing, `FindProxyForURL` generation and `file_url` |
| `policy.rs` | `Config::captured` (scope, exclude, mute against the full URL), `should_decrypt`, `matching`; URL and glob matching |
| `app.rs` | `AppIdentity` and pure helpers: `.app` bundle from an executable path, Info.plist fields, simulator and emulator detection, icon cache keys, letter-avatar slot |
| `filter.rs` | search query parser |
| `mime.rs` | file extension to content type |
| `repaint.rs` | `Repaint` trait |
| `about.rs` | app name, version, author and license for the About window |

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
| `app_icons.rs` | resolves the client process to an app (name, bundle id) and extracts its icon to `<data dir>/icons/` (`.icns` and PNG decoding in Rust, no `sips`); `IconStore` serves PNG bytes from memory, loading from disk on a worker thread |
| `devices.rs` | `apply_route`, CA trust, Chrome, iOS simulator, adb |
| `app_menu.rs` | native menu bar on macOS and Windows (`muda`); none on Linux, where the toolbar draws Help |
| `sysproxy/` | system proxy (global, or PAC for proxy-only hosts) per OS: `macos.rs`, `linux.rs`, `windows.rs`, `other.rs` |
| `car.rs` | `Assets.car` reader: BOM trees, `AppIcon` facet, largest rendition as PNG (embedded PNG, raw, zlib, LZFSE/LZVN `KCBC`; RLE and palette skipped) |
| `android_icons.rs`, `android_pkg.rs`, `zip_read.rs` | package id to label and icon via adb: command runner, APK zip reader, binary `AndroidManifest.xml` and `resources.arsc` parsers |
| `client_proc.rs` | client process (name, pid) and executable path per OS |
| `os_parse.rs` | parsers for `lsof`, `/proc/net/tcp`, `netstat`, `tasklist`, `gsettings`, `reg`, `.desktop` and `.icns` |
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
| `app_cache.rs` | one client-process lookup per client address (60s), shared across the per-request handler clones |
| | The client process is found with `services::client_proc` on a background thread, then resolved with `app_icons::resolve` |

### `viewmodels/`
| File | Holds |
|---|---|
| `capture_vm.rs` | filter, sort, selection, visible rows, sidebar tree; row, sidebar and shortcut intents |
| `detail_vm.rs` | tabs, view modes, diff base and cache |
| `rules_vm.rs` | rule form, breakpoints, console, scope lists, `RuleKind` |
| `replay_vm.rs` | replay and compose drafts |
| `network_vm.rs`, `schemas_vm.rs` | the Network and Schemas tabs of Settings |
| `settings_vm.rs` | Settings window visibility and tab |
| `about_vm.rs` | About window visibility |
| `session_vm.rs` | autosave, session and rules files |
| `devices_vm.rs` | device actions; simulator and system-proxy state polled off the UI thread |
| `onboarding_vm.rs` | onboarding stages; device and import intents become `Effect`s |
| `status.rs` | status line, `JobRunner` for background jobs |
| `intent.rs` | intent and effect enums |

### `views/`
One file per screen area: `toolbar`, `sidebar`, `table`, `detail`, `rules_panel`, `breakpoint_window`, `replay_window`, `settings_window` (with `network_tab` and `schemas_tab`), `about_window`, `onboarding_window` (first-run setup guide), `shortcuts`, `body`. Also `theme` and `icon`. `app_icon` turns icon PNG bytes from the viewmodel's `IconStore` into cached egui textures and draws the letter avatar fallback.

## Extending

- A rule type implements `proxy::actions::RuleAction`. The pipeline only knows the trait.
- A body view implements `services::decoders::BodyDecoder` and returns a `Rendered` value (text, form, tree, hex or sections). Views draw `Rendered` and never match on a decoder.

Neither should panic. They return values or change the message they are given.
