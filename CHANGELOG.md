# Changelog

## Unreleased

### Added

- Scope entries can be URL patterns such as `api.example.com/v2/*` or `https://*.example.com/graphql`, and a new Exclude list hides matching URLs from the record. Saved host-only scopes keep working.
- The request context menu has Scope to this host, Scope to this URL path and Exclude this URL; the Scope tab shows how many recorded requests match, and the toolbar shows a Scoped pill while scope or exclude is active.

### Changed

- Compose, Replay and the breakpoint editor open as native windows instead of floating inside the app. Cmd+Enter sends or executes; the breakpoint window comes to the front when traffic pauses.
- Network and Schemas move from the Rules panel to a Settings window (Cmd+,).
- The dark theme is true black for OLED screens.
- The toolbar's Clear keeps pinned requests; Clear all, including pinned, is in the Session menu.
- The devices menu is now called Connect, and the sidebar toggle is an icon.
- Rule types are picked from a dropdown, and Add rule is the form's primary button.

### Fixed

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
