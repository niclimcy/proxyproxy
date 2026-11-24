# Changelog

## v0.3.0

Changes since [v0.2.0](https://github.com/niclimcy/proxyproxy/releases/tag/v0.2.0).

### Added

- App icons: each flow resolves its client process to an app bundle, so the table's App column and the detail header show the real name and icon, including iOS Simulator apps instead of `launchd_sim`. Apps without an extractable icon, and the Android emulator, get a letter avatar. Clicking an icon filters to that app (`app:`), and clicking again clears it. Old `.pxp` sessions still load.
- Scope entries can be URL patterns such as `api.example.com/v2/*` or `https://*.example.com/graphql`, and a new Exclude list hides matching URLs from the record. Saved host-only scopes keep working.
- The request context menu has Scope to this host, Scope to this URL path and Exclude this URL; the Scope tab shows how many recorded requests match, and the toolbar shows a Scoped pill while scope or exclude is active.

- Proxy-only hosts: a Scope tab list (plus Add hosts from Scope) and a Connect menu toggle route only those hosts through the system proxy using a generated PAC file; other traffic connects directly. The request context menu has Route only this host through proxy and Add to proxy-only list, and the toolbar shows an N hosts only pill. Works on macOS, Linux (GNOME) and Windows; edits rewrite the PAC while the proxy is on, and already-open connections keep their route.

- App icons for iOS Simulator apps are read from the compiled `Assets.car` (pure Rust, no `actool`/`assetutil`), falling back to loose `AppIcon*.png`. Android emulator and USB device flows resolve a package id from `X-Requested-With` or the User-Agent, then get the app label and icon through `adb` (APK pulled once, cached, skipped silently when adb or the package is missing).
- The detail panel closes with a button, Esc or by dragging it shut.
- Cmd+Shift+P toggles recording.
- HTTP/2 is enabled in the proxy.

### Changed

- Cross-platform: the client app lookup works on Linux (`/proc`) and Windows (`netstat`, `tasklist`), Linux apps get names and icons from `.desktop` files, and the system proxy and PAC routing support GNOME `gsettings` and the Windows registry. Icons are decoded in Rust, so `sips` is no longer needed. Unsupported setups show a hint instead of failing.

- Compose, Replay and the breakpoint editor open as native windows instead of floating inside the app. Cmd+Enter sends or executes; the breakpoint window comes to the front when traffic pauses.
- Network and Schemas move from the Rules panel to a Settings window (Cmd+,).
- The dark theme is true black for OLED screens. Labelled buttons get a quiet filled surface with a soft hairline, and grouped toolbar actions sit in one capsule.
- The toolbar's Clear keeps pinned requests; Clear all, including pinned, is in the Session menu.
- The devices menu is now a Sources menu: the recording pill only says Recording/Paused, the menu button names the active routes, and system proxy and host-only routing are checkbox rows. The sidebar toggle is an icon.
- Rule types are picked from a dropdown, and Add rule is the form's primary button.

### Fixed

- Panel seams get a single separator instead of doubled outlines, and the no-match card stays centered when the filter is long.
- Menu items, action buttons and choice cards expose button roles and labels to assistive tech.
- Web pages can no longer change rules or replay requests through the MCP endpoint: posts must be JSON, cross-site `Origin`s and non-loopback `Host`s are refused, and bodies are capped at 4 MB.
- Quitting only turns off the system proxy when it points at proxyproxy, so a proxy you set up yourself is left alone.
- iOS simulator and system proxy status are polled in the background instead of blocking the UI every second, and the onboarding screen now updates while open.
- Map-to-local editing refuses files over 1 MB and won't save into a different file than the one loaded.
- Existing users are no longer shown onboarding after upgrading.
- SOCKS5: percent-encoded credentials, IP-literal targets and the 1080 default port; PAC `SOCKS` (SOCKS4) entries are skipped.
- The Path column stays visible when the table is narrow, and a selected row keeps its badges readable.
- Menus and small actions use one style throughout; tab widths, key/value headers and the detail URL line up.
- MCP: unknown resources return a JSON-RPC error, a malformed stdio line no longer ends the session, and a slow replay no longer blocks other API calls.

## v0.2.0

Changes since [v0.1.0](https://github.com/niclimcy/proxyproxy/releases/tag/v0.1.0).

### Added

- MCP support over HTTP JSON-RPC and stdio, including tools and resources for captured traffic, flows, rules, request replay, and HAR export.
- SOCKS5 upstream proxy support.
- An onboarding flow for initial proxy and certificate setup.
- Editing map-to-local rules directly in the rules panel.
- Syntax highlighting for editable rule content.

### Improved

- iOS Simulator certificate setup and installation.
- UI consistency across the capture, detail, replay, rules, schemas, and toolbar views.
- macOS DMG installation guidance, including an Applications shortcut and drag-to-install background.

### Fixed

- The system proxy is cleared when the app closes.
