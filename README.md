# oh-my-grok (`omg`)

**oh-my-grok is an independent, unofficial multi-provider fork of
[Grok Build](https://github.com/xai-org/grok-build). It is not published,
maintained, endorsed, or supported by xAI or SpaceXAI.**

It keeps the upstream X account flow and adds ChatGPT OAuth, Codex Responses
support, and the GPT-5.6 Sol, Terra, and Luna model family. The primary binary
is `omg`, with `oh-my-grok` as a long-name alias.

oh-my-grok is a terminal-based AI coding agent. It runs as a
full-screen TUI that understands your codebase, edits files, executes shell
commands, searches the web, and manages long-running tasks — interactively,
headlessly for scripting/CI, or embedded in editors via the Agent Client
Protocol (ACP).

[Installing the released binary](#installing-the-released-binary) ·
[Building from source](#building-from-source) ·
[Documentation](#documentation) ·
[Repository layout](#repository-layout) ·
[Development](#development) ·
[Contributing](#contributing) ·
[License](#license)

This repository contains the Rust source for the `omg` CLI/TUI and its agent
runtime. It is derived from the upstream source published at
[`xai-org/grok-build`](https://github.com/xai-org/grok-build); upstream names,
protocol identifiers, and copyright notices are retained where attribution or
compatibility requires them.

A small `SOURCE_REV` file at the root records the full monorepo commit SHA
for the version of the code present in this tree.

---

## Installing the released binary

Install the latest stable release on Linux x86_64:

```sh
curl -fsSL https://github.com/estridell/oh-my-grok/releases/latest/download/install.sh | bash
```

To install a specific version, pass it as the first script argument:

```sh
curl -fsSL https://github.com/estridell/oh-my-grok/releases/latest/download/install.sh | bash -s 0.1.1
```

The installer verifies the release checksum, smoke-tests the binary, and
installs `omg` and `oh-my-grok` under `~/.oh-my-grok/bin`. New stable GitHub
Releases are detected by the CLI within its normal 30-minute update-check
window. Other platforms and npm distribution are not active for future v1
releases while the fork is single-user.

## Building from source

Requirements:

- **Rust** — the toolchain is pinned by [`rust-toolchain.toml`](rust-toolchain.toml);
  `rustup` installs it automatically on first build.
- **[DotSlash](https://dotslash-cli.com)** — required so hermetic tools under
  [`bin/`](bin/) (notably [`bin/protoc`](bin/protoc)) can download and run.
  Install it and ensure `dotslash` is on your `PATH` **before** building:

  ```sh
  cargo install dotslash
  # or: prebuilt packages — https://dotslash-cli.com/docs/installation/
  /usr/bin/env dotslash --help   # sanity check
  ```

- **protoc** — proto codegen resolves [`bin/protoc`](bin/protoc) via DotSlash,
  or falls back to a `protoc` on `PATH` / `$PROTOC`.
- macOS and Linux are supported build hosts; Windows builds are best-effort
  and not currently tested from this tree.

```sh
cargo run -p xai-grok-pager-bin --bin omg    # build + launch the TUI
cargo build -p xai-grok-pager-bin --profile release-dist --features release-dist
cargo check -p xai-grok-pager-bin            # fast validation
```

The primary binary is `omg`, and the long-name alias is `oh-my-grok`. State is
stored in `~/.oh-my-grok`; `OH_MY_GROK_HOME` overrides that path and the legacy
`GROK_HOME` variable remains supported. The TUI starts without requiring a
provider; use `/login xai` for X or `/login chatgpt` for ChatGPT OAuth. See the
[authentication guide](crates/codegen/xai-grok-pager/docs/user-guide/02-authentication.md).

## Documentation

The upstream Grok Build documentation is available at
[docs.x.ai/build/overview](https://docs.x.ai/build/overview). It is maintained
by xAI and may describe behavior that differs from this fork.

The user guide ships with the pager crate:
[`crates/codegen/xai-grok-pager/docs/user-guide/`](crates/codegen/xai-grok-pager/docs/user-guide/)
— getting started, keyboard shortcuts, slash commands, configuration, theming,
MCP servers, skills, plugins, hooks, headless mode, sandboxing, and more.

## Privacy & telemetry

**oh-my-grok does not phone home to xAI/X.** No analytics, usage events,
diagnostics, crash reports, traces, or product telemetry are sent by default,
and network-delivered remote configuration cannot re-enable them. Crash reports
stay local. See [`PRIVACY.md`](PRIVACY.md) for the full network behavior and the
xAI/X provider traffic that is retained for operations you explicitly request.

## Repository layout

| Path | Contents |
|------|----------|
| `crates/codegen/xai-grok-pager-bin` | Composition-root package; builds the `omg` and `oh-my-grok` binaries |
| `crates/codegen/xai-grok-pager` | The TUI: scrollback, prompt, modals, rendering |
| `crates/codegen/xai-grok-shell` | Agent runtime + leader/stdio/headless entry points |
| `crates/codegen/xai-grok-tools` | Tool implementations (terminal, file edit, search, ...) |
| `crates/codegen/xai-grok-workspace` | Host filesystem, VCS, execution, checkpoints |
| `crates/codegen/...` | The rest of the CLI crate closure (config, MCP, markdown, sandbox, ...) |
| `crates/common/`, `crates/build/`, `prod/mc/` | Small shared leaf crates pulled in by the closure |
| `third_party/` | Vendored upstream source (Mermaid diagram stack) — see below |

> [!IMPORTANT]
> The root `Cargo.toml` (workspace members, dependency versions, lints,
> profiles) is **generated** — treat it as read-only. Prefer editing per-crate
> `Cargo.toml` files.

## Development

```sh
cargo check -p <crate>        # always target specific crates; full-workspace builds are slow
cargo test -p xai-grok-config # per-crate tests
cargo clippy -p <crate>       # lint config: clippy.toml at the repo root
cargo fmt --all               # rustfmt.toml at the repo root
```

Maintainers publish releases by pushing a stable `vX.Y.Z` tag. See
[`RELEASING.md`](RELEASING.md) for the build matrix, artifacts, and update
propagation behavior.

## Contributing

> [!NOTE]
> External contributions are not accepted. See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

First-party code in this repository is licensed under the **Apache License,
Version 2.0** — see [`LICENSE`](LICENSE).

Third-party and vendored code remains under its original licenses. See:

- [`THIRD-PARTY-NOTICES`](THIRD-PARTY-NOTICES) — crates.io / git dependencies,
  bundled UI themes, and **in-tree source ports** (including openai/codex and
  sst/opencode tool implementations)
- [`crates/codegen/xai-grok-tools/THIRD_PARTY_NOTICES.md`](crates/codegen/xai-grok-tools/THIRD_PARTY_NOTICES.md)
  — crate-local notice for the codex and opencode ports (license texts +
  Apache §4(b) change notice)
- [`third_party/NOTICE`](third_party/NOTICE) — vendored Mermaid-stack index
