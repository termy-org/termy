# macOS Experiments

This directory is for temporary patches and helper scripts used while testing
native macOS host changes. Keep production Swift source under
`macos/Sources/TermySwift/`, and promote durable work into normal source,
scripts, or docs before release.

The production native DMG entrypoint is `macos/scripts/build-dmg.sh`.

`build-gpui-dmg-signed.sh` is the old cargo-bundle signing path for the GPUI
app shell. It is archived here because the native Swift DMG path is now the
canonical macOS packaging route.
