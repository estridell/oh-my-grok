# npm distribution is inactive

The inherited npm packaging is retained for possible future use, but it is
not part of the oh-my-grok v1 release path. Every package is marked `private`
so `npm publish` refuses to publish it, and the CLI updater does not select the
npm backend.

GitHub Releases and `install.sh` are the only supported binary distribution
path for v1. Remove the `private` guards and re-enable updater selection only
as part of an intentional npm launch with renamed, fork-owned packages.
