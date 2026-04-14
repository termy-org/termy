# Termy Shell Integration for Bash
# This file should be sourced in your ~/.bashrc or ~/.bash_profile
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

__termy_prompt_initialized=0
__termy_prompt_command() {
    local exit_code=$?
    # Only emit D marker after the first prompt (not on shell startup)
    if [[ "$__termy_prompt_initialized" == "1" ]]; then
        # Report command finished with exit code
        printf '\e]133;D;%d\a' "$exit_code"
    fi
    __termy_prompt_initialized=1
    # Report prompt start
    printf '\e]133;A\a'
}

# Prepend our function to PROMPT_COMMAND
if [[ -n "$PROMPT_COMMAND" ]]; then
    PROMPT_COMMAND="__termy_prompt_command;$PROMPT_COMMAND"
else
    PROMPT_COMMAND="__termy_prompt_command"
fi

# Track command execution via DEBUG trap
__termy_in_command=0
__termy_debug_trap() {
    # Skip if this is our own prompt command
    if [[ "$BASH_COMMAND" == "__termy_prompt_command"* ]]; then
        return
    fi
    # Only emit once per command, and only in top-level shell (not subshells)
    if [[ "$__termy_in_command" == "0" && "$BASH_SUBSHELL" == "0" ]]; then
        __termy_in_command=1
        printf '\e]133;C\a'
    fi
}
trap '__termy_debug_trap' DEBUG

# Reset command flag after prompt
__termy_reset_command_flag() {
    __termy_in_command=0
    # Report command input start (B marker)
    printf '\e]133;B\a'
}
PROMPT_COMMAND="${PROMPT_COMMAND};__termy_reset_command_flag"

# OSC 7: Report current working directory
__termy_urlencode_path() {
    local path="$1"
    local encoded=""
    local i char
    for ((i = 0; i < ${#path}; i++)); do
        char="${path:i:1}"
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
    printf '\e]7;file://%s%s\a' "${HOSTNAME:-$(hostname)}" "$encoded_path"
}
PROMPT_COMMAND="${PROMPT_COMMAND};__termy_report_cwd"

# Report initial directory
__termy_report_cwd
