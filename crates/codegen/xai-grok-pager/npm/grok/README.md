# OMG

OMG is the CLI for
[oh-my-grok](https://github.com/estridell/oh-my-grok), an independent,
unofficial multi-provider fork of Grok Build. It is not published, maintained,
endorsed, or supported by xAI or SpaceXAI.

## Install

The fork does not currently publish an npm package. Install the Linux x86_64
release from GitHub:

```bash
curl -fsSL https://github.com/estridell/oh-my-grok/releases/latest/download/install.sh | bash
```

## Get Started

```bash
# Launch the interactive TUI
omg

# Run a single task
omg -p "Explain this codebase"
```

OMG starts without requiring a provider. Use `/login xai` for X or
`/login chatgpt` for ChatGPT OAuth.

```bash
omg update
```

## Documentation

See the [bundled user guide](../../docs/user-guide/README.md). Upstream Grok
Build documentation may describe behavior that differs from OMG.

## Feedback

Run `/feedback` inside OMG to report issues.
