# AGENTS.md

proxyproxy is an HTTP(S) debugging proxy with an egui UI. Single Rust crate.

## Commands

```sh
cargo build
cargo test     # 195 tests, no window needed
cargo run -- 19090   # proxy on :19090, REST API on :19091
```

A change is done when both build and test pass with no warnings.

## Layout

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). In short:

- `domain/`: data and policy
- `services/`: file, process and network helpers
- `proxy/`: the MITM proxy
- `viewmodels/`: UI state and intent handlers
- `views/`: egui drawing
- `app.rs`: builds everything and runs the UI

## Rules

- Only `views/` and `app.rs` import `egui`/`eframe`. Check: `grep -rln "egui\|eframe" src | grep -v "^src/views/\|^src/app.rs"` should print only `src/main.rs`.
- Views return intents (`viewmodels/intent.rs`). They don't decide behaviour.
- Styling goes through tokens in `views/theme.rs` (see [docs/DESIGN.md](docs/DESIGN.md)). No raw `Color32::from_rgb` or literal font sizes in views.
- Don't hold the `FlowStore` lock across `.await`.
- New rule type: implement `RuleAction` in `proxy/actions/`. New body view: implement `BodyDecoder` in `services/decoders/`. Steps are in [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md).
- Don't change these behaviours (tests cover them): rule edits apply to the next request; "don't decrypt" applies to new connections only; mute and scope only affect what is recorded.
- Add tests next to the code. ViewModel tests need no egui types.
- Comment only non-obvious reasons, in a short phrase.
