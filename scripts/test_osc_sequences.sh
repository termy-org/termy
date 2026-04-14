#!/bin/bash
# Test script for OSC escape sequences
# Run this in Termy to verify notification and progress support

set -e

echo "=== Testing OSC Escape Sequences ==="
echo ""

# OSC 133: Shell Integration (Command Lifecycle)
echo "--- OSC 133: Shell Integration ---"
echo "Simulating command lifecycle..."
printf '\e]133;A\a'  # Prompt start
sleep 0.2
printf '\e]133;B\a'  # Command input
sleep 0.2
printf '\e]133;C\a'  # Command executing
echo "  [Command running for 1 second...]"
sleep 1
printf '\e]133;D;0\a'  # Command finished (exit code 0)
echo "  OSC 133 sequence complete (exit code 0)"
echo ""

# OSC 9: Simple Notification
echo "--- OSC 9: Simple Notification ---"
echo "Sending notification..."
printf '\e]9;This is a test notification from Termy!\a'
echo "  Sent: 'This is a test notification from Termy!'"
sleep 1
echo ""

# OSC 777: Notification with Title
echo "--- OSC 777: Notification with Title ---"
echo "Sending notification with title..."
printf '\e]777;notify;Build Complete;Your project has been built successfully.\a'
echo "  Sent: title='Build Complete', body='Your project has been built successfully.'"
sleep 1
echo ""

# OSC 9;4: Progress Indicator
echo "--- OSC 9;4: Progress Indicator ---"
echo "Showing progress from 0% to 100%..."
for i in $(seq 0 10 100); do
    printf '\e]9;4;1;%d\a' "$i"
    printf "\r  Progress: %3d%%" "$i"
    sleep 0.1
done
echo ""
echo "Clearing progress..."
printf '\e]9;4;0;0\a'
sleep 0.5
echo ""

# OSC 9;4: Indeterminate Progress
echo "--- OSC 9;4: Indeterminate Progress ---"
echo "Showing indeterminate progress for 2 seconds..."
printf '\e]9;4;3;0\a'
sleep 2
printf '\e]9;4;0;0\a'
echo "  Done"
echo ""

# OSC 9;4: Error State
echo "--- OSC 9;4: Error State ---"
echo "Showing error state for 1 second..."
printf '\e]9;4;2;75\a'
sleep 1
printf '\e]9;4;0;0\a'
echo "  Done"
echo ""

# OSC 9;4: Warning State
echo "--- OSC 9;4: Warning State ---"
echo "Showing warning state for 1 second..."
printf '\e]9;4;4;50\a'
sleep 1
printf '\e]9;4;0;0\a'
echo "  Done"
echo ""

# OSC 7: Working Directory
echo "--- OSC 7: Working Directory ---"
echo "Reporting current directory..."
printf '\e]7;file://localhost%s\a' "$PWD"
echo "  Sent: file://localhost$PWD"
echo ""

echo "=== All OSC tests complete! ==="
echo ""
echo "Expected behaviors:"
echo "  - OSC 133: Terminal tracks command lifecycle (visible in debug mode)"
echo "  - OSC 9/777: Desktop notification appears (if window unfocused)"
echo "  - OSC 9;4: Tab shows progress indicator"
echo "  - OSC 7: Working directory updates in terminal state"
