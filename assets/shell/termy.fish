# Termy Shell Integration for Fish
# This file should be sourced in your ~/.config/fish/config.fish
# It enables OSC 133 shell integration for command lifecycle tracking

if test "$TERMY_SHELL_INTEGRATION" != "1"
    exit
end

# Prevent double-sourcing
if set -q __termy_shell_integration_loaded
    exit
end
set -g __termy_shell_integration_loaded 1

# OSC 133 markers:
# A = Prompt start (shell is displaying prompt)
# B = Command input start (user is typing)
# C = Command executing (command has been submitted)
# D;code = Command finished with exit code

function __termy_prompt --on-event fish_prompt
    # Report command finished with exit code
    printf '\e]133;D;%d\a' $status
    # Report prompt start
    printf '\e]133;A\a'
    # Report command input start (ready for typing)
    printf '\e]133;B\a'
end

function __termy_preexec --on-event fish_preexec
    # Report command executing
    printf '\e]133;C\a'
end

# OSC 7: Report current working directory after execution
function __termy_postexec --on-event fish_postexec
    printf '\e]7;file://%s%s\a' (hostname) $PWD
end

# Report initial directory
printf '\e]7;file://%s%s\a' (hostname) $PWD
