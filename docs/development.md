# Development

## Tmux integration tests

Run the local end-to-end tmux split integration harness:

```sh
just test-tmux-integration
```

Requirements:
- tmux `>= 3.3`

Optional:
- Override tmux binary path with `TERMY_TEST_TMUX_BIN=/path/to/tmux`
