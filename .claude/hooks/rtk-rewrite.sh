#!/bin/bash
# RTK rewrite hook for Claude Code — installed by indxr init
# Intercepts Bash commands and rewrites them through rtk for token compression

# Skip silently if rtk or jq is not installed
command -v rtk >/dev/null 2>&1 || exit 0
command -v jq >/dev/null 2>&1 || exit 0

# Extract the command from tool input
COMMAND=$(printf '%s' "$TOOL_INPUT" | jq -r '.command // empty')
[ -z "$COMMAND" ] && exit 0

# Ask rtk to rewrite the command
REWRITTEN=$(rtk rewrite "$COMMAND" 2>/dev/null)
EXIT_CODE=$?

case $EXIT_CODE in
  0)
    # Rewrite successful — auto-allow with rewritten command
    ESCAPED=$(printf '%s' "$REWRITTEN" | jq -Rs .)
    echo "{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"allow\",\"updatedInput\":{\"command\":$ESCAPED}}}"
    ;;
  2)
    # Deny rule matched
    echo "{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"deny\"}}"
    ;;
  3)
    # Ask rule matched
    echo "{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"ask\"}}"
    ;;
  *)
    # No rewrite available or error — pass through unchanged
    exit 0
    ;;
esac
