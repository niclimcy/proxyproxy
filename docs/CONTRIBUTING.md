# Contributing

Layout and layer boundaries are in [ARCHITECTURE.md](ARCHITECTURE.md).

## Build and test

```sh
cargo build
cargo test
```

Both should pass with no warnings. Tests don't need a window or network, apart from the proxy and upstream tests, which use loopback.

To try it by hand:

```sh
cargo run -- 19090
curl -x http://127.0.0.1:19090 http://example.com/
curl -s http://127.0.0.1:19091/flows     # REST API, port + 1
```

After changing `proxy/` or `views/`, check HTTP and HTTPS capture, a breakpoint edit, replay, mute and scope, and HAR and Charles import.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): summary`. The scope is optional.

| Type | Use for |
|---|---|
| `feat` | new behaviour or UI users can see |
| `fix` | a bug fix |
| `refactor` | code change with no behaviour change |
| `test` | adding or changing tests only |
| `chore` | tooling, dependencies, docs, other upkeep |

- Keep the summary short and lower case, with no trailing period.
- Mark breaking changes with `!` after the type (`feat!:`) and a `BREAKING CHANGE:` footer.
- Keep each commit to one type. Split a fix from a refactor.
- Don't add `Co-Authored-By` trailers.

## Where a change goes

- Data or policy: `domain/`.
- File, network or process I/O: `services/`, or `proxy/` if it's on the proxy path.
- What a click does: a viewmodel intent handler.
- Drawing: `views/`.

Don't import `egui` or `eframe` outside `views/` and `app.rs`. To check:

```sh
grep -rln "egui\|eframe" src | grep -v "^src/views/\|^src/app.rs"
```

Only `main.rs` should print, for the `eframe::Result` return type.

## Adding an intent

1. Add a variant to the enum in `viewmodels/intent.rs`.
2. Handle it in the viewmodel's `handle*` method. If another viewmodel has to react, return an `Effect` and route it in `App::apply`.
3. Emit it from the view.
4. Test the handler without egui. `capture_vm.rs` has examples.

## Adding a rule action

1. `domain/rule.rs`: add the `Action` variant, with `label` and `describe`.
2. `proxy/actions/<name>.rs`: implement `RuleAction`, overriding only the hooks you need, and add tests. Add a line to `resolve` in `proxy/actions/mod.rs`. Add the variant to `every_action_variant_resolves_with_expected_tag`.
3. `viewmodels/rules_vm.rs`: `RuleKind`, `KINDS`, the `RuleForm` fields, `from_rule`, `valid`, `action`. Add the variant to `form_roundtrips_every_action_kind`.
4. `views/rules_panel.rs`: the hint text and form fields.

## Adding a body view

1. `services/decoders/<name>.rs`: implement `BodyDecoder` and return a `Rendered` value. Set `default_rank` only if it should be the default when it applies (lower wins).
2. Add it to `ALL` in `services/decoders/mod.rs`. The order there is the tab order.
3. Add a `Rendered` variant and a branch in `views/body.rs` only if no existing variant fits.

## Behaviour covered by tests

- Rule edits apply to the next request.
- "Don't decrypt" applies to new connections only.
- Mute and scope affect what is recorded, not the network.
- Pinned flows survive the 20k cap and "Clear".
- A breakpoint body that wasn't edited is forwarded byte for byte.
- Dropping a pending breakpoint resumes the request.

## Tests

- Put unit tests next to the code in `#[cfg(test)] mod tests`.
- Viewmodel tests use `Flow::sample`, a `FlowStore` and `status::NoRepaint`. Don't create egui types.
- Pipeline functions are plain functions; call them directly.
- Files written to the temp dir need unique names. Include the process id.

## Style

- Comment only non-obvious reasons, such as a workaround or an invariant. One short phrase.
- Keep lock guards short and block-scoped.
- When restructuring, move code first and change it in a later commit.
