# Changelog

## Unreleased

### Fixed

- Web pages can no longer change rules or replay requests through the MCP endpoint: posts must be JSON, cross-site `Origin`s and non-loopback `Host`s are refused, and bodies are capped at 4 MB.
- Quitting only turns off the system proxy when it points at proxyproxy, so a proxy you set up yourself is left alone.
- iOS simulator and system proxy status are polled in the background instead of blocking the UI every second, and the onboarding screen now updates while open.
- Map-to-local editing refuses files over 1 MB and won't save into a different file than the one loaded.
- Existing users are no longer shown onboarding after upgrading.
- SOCKS5: percent-encoded credentials, IP-literal targets and the 1080 default port; PAC `SOCKS` (SOCKS4) entries are skipped.
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
