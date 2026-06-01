# Pull Request

<!-- A clear and concise description of what this PR does and why it's needed. -->

## Description

<!-- Describe the changes made in the PR. -->

- 
-
-
-

## Screenshots/Videos

<!-- If applicable, add screenshots or screen recordings to help explain your changes. -->


## Related Issues

<!-- If this PR closes any issues, use the keyword 'closes' followed by the issue number -->

Closes #

## Checklist

- [ ] I confirmed there is no existing open PR for the same or overlapping changes.
- [ ] I ran the smallest validation pass for this change (see [CONTRIBUTING.md](CONTRIBUTING.md#testing-and-validation)).
- [ ] `just check-boundaries` (if deps, config keys, commands, or generated docs changed).
- [ ] `just test-workspace` or targeted `cargo test -p <crate>` (if Rust behavior changed).
- [ ] `just fmt-check` (if Rust formatting may have drifted).
- [ ] `just test-tmux-integration` (if tmux behavior changed).
- [ ] Regenerated `docs/configuration.md` / `docs/keybindings.md` when schema or defaults changed.
