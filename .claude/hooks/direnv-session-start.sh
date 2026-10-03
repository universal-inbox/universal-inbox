#!/bin/sh
# SessionStart hook: make every Bash command of this session load the worktree's
# direnv env through direnv-env.sh (see there for why).
[ -n "$CLAUDE_ENV_FILE" ] && [ -n "$CLAUDE_PROJECT_DIR" ] || exit 0

# Drop caches from old sessions
find "$CLAUDE_PROJECT_DIR/.direnv" -name 'claude-env.*' -mtime +7 -delete 2>/dev/null

cat >>"$CLAUDE_ENV_FILE" <<EOF
UI_DIRENV_ROOT='$CLAUDE_PROJECT_DIR'
. '$CLAUDE_PROJECT_DIR/.claude/hooks/direnv-env.sh'
EOF
