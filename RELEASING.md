# Releasing oh-my-grok

oh-my-grok v1 currently publishes a Linux x86_64 binary through GitHub
Releases. Other platforms, npm, and Windows packaging are intentionally
inactive while the fork is single-user.

## Publish a release

1. Make sure `main` is clean and the intended commit has passed its checks.
2. Choose a new stable semantic version and push its tag:

   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

3. The `Release` workflow validates the tag and builds the hardened
   `linux-x86_64` `omg` binary on a GitHub-hosted runner.

4. After the build passes its `--version` smoke test, the workflow creates a
   draft release, uploads the binary plus `install.sh`, `version`, and
   `SHA256SUMS`, then publishes it as the latest release.

The tag version is injected at compile time with `GROK_VERSION`; inherited
crate versions do not need to be rewritten for each fork release. Distribution
builds use the workspace's hardened `release-dist` profile and feature.

## Installation and updates

The stable installation URL is:

```bash
curl -fsSL https://github.com/estridell/oh-my-grok/releases/latest/download/install.sh | bash
```

The installer first reads `releases/latest/download/version`, then downloads
and verifies the corresponding version-pinned binary. Installed clients use
the same version asset and normally check it at most once every 30 minutes.
Publishing the release therefore makes the update available without a
separate channel-pointer deployment.

Drafts, prereleases, and failed workflows never replace the latest stable
release. If a release workflow fails after creating its draft, delete that
draft before rerunning the workflow for the same tag.
