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

__termy_precmd_initialized=0
__termy_precmd() {
    local exit_code=$?
    # Only emit D marker after the first prompt (not on shell startup)
    if [[ "$__termy_precmd_initialized" == "1" ]]; then
        # Report command finished with exit code
        printf '\e]133;D;%d\a' "$exit_code"
    fi
    __termy_precmd_initialized=1
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
# Preserve any existing zle-line-init widget
if (( ${+widgets[zle-line-init]} )); then
    zle -A zle-line-init __termy_orig_zle_line_init
fi
__termy_zle_line_init() {
    printf '\e]133;B\a'
    # Call original widget if it existed
    if (( ${+widgets[__termy_orig_zle_line_init]} )); then
        zle __termy_orig_zle_line_init
    fi
}
zle -N zle-line-init __termy_zle_line_init

# OSC 7: Report current working directory on directory change
__termy_urlencode_path() {
    local path="$1"
    local encoded=""
    local i char
    for ((i = 1; i <= ${#path}; i++)); do
        char="${path[i]}"
        case "$char" in
            [a-zA-Z0-9._~/-]) encoded+="$char" ;;
            *) encoded+=$(printf '%%%02X' "'$char") ;;
        esac
    done
    printf '%s' "$encoded"
}
__termy_report_cwd() {
    local encoded_path
    encoded_path=$(__termy_urlencode_path "$PWD")
    printf '\e]7;file://%s%s\a' "${HOST:-$(hostname)}" "$encoded_path"
}
chpwd_functions+=(__termy_report_cwd)

# Report initial directory
__termy_report_cwd
