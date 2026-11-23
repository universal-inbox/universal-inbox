#!/bin/sh
[ -n "$CLAUDE_ENV_FILE" ] && [ -n "$CLAUDE_PROJECT_DIR" ] || exit 0
command -v mise >/dev/null 2>&1 || exit 0

cat >>"$CLAUDE_ENV_FILE" <<EOF
eval "\$(mise env -C '$CLAUDE_PROJECT_DIR' -s bash 2>/dev/null)"
EOF
