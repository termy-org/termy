# Termy Shell Integration for Zsh
# This file should be sourced in your ~/.zshrc
# It enables OSC 133 shell integration for command lifecycle tracking

if [[ "$TERMY_SHELL_INTEGRATION" != "1" ]]; then
    return
fi

# Prevent double-sourcing
if [[ -n "$__termy_shell_integration_loaded" ]]; then
    return
fi
__termy_shell_integration_loaded=1

# OSC 133 markers:
# A = Prompt start (shell is displaying prompt)
# B = Command input start (user is typing)
# C = Command executing (command has been submitted)
# D;code = Command finished with exit code

__termy_precmd() {
    local exit_code=$?
    # Report command finished with exit code
    printf '\e]133;D;%d\a' "$exit_code"
    # Report prompt start
    printf '\e]133;A\a'
}

__termy_preexec() {
    # Report command executing
    printf '\e]133;C\a'
}

# Hook into zsh's precmd and preexec
precmd_functions+=(__termy_precmd)
preexec_functions+=(__termy_preexec)

# Report command input start when line editor initializes
__termy_zle_line_init() {
    printf '\e]133;B\a'
}
zle -N zle-line-init __termy_zle_line_init

# OSC 7: Report current working directory on directory change
__termy_report_cwd() {
    printf '\e]7;file://%s%s\a' "${HOST:-$(hostname)}" "$PWD"
}
chpwd_functions+=(__termy_report_cwd)

# Report initial directory
__termy_report_cwd
