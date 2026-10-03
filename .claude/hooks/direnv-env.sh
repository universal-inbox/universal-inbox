# Sourced (bash or zsh) before every Claude Code Bash command via $CLAUDE_ENV_FILE,
# see direnv-session-start.sh. Requires UI_DIRENV_ROOT (the worktree root).
#
# A herdr/flatcircle pane can start before `wt hook pre-start` writes .local_envrc
# and runs `direnv allow`: the session env snapshot then holds a blocked direnv load
# and misses every .envrc export. Re-evaluating .envrc on each command costs seconds
# (devbox), so cache `direnv export` and rebuild it only when an input changes.
# The export is a diff against the snapshot env, so the cache is keyed by the
# snapshot's DIRENV_DIFF. An empty cache is valid: that snapshot was up to date.
# While .envrc is still blocked, do nothing: the next command retries.

_ui_direnv_load() {
  local root="$UI_DIRENV_ROOT" shell key cache f
  [ -n "$root" ] && [ -f "$root/.envrc" ] || return 0
  command -v direnv >/dev/null 2>&1 || return 0

  if [ -n "$ZSH_VERSION" ]; then shell=zsh; else shell=bash; fi
  key=$(printf '%s' "$DIRENV_DIFF" | cksum | cut -d' ' -f1)
  cache="$root/.direnv/claude-env.$key.$shell"

  if [ -e "$cache" ]; then
    for f in .envrc .local_envrc .env devbox.json devbox.lock; do
      if [ -e "$root/$f" ] && [ "$root/$f" -nt "$cache" ]; then
        rm -f "$cache"
        break
      fi
    done
  fi

  if [ ! -e "$cache" ]; then
    # `direnv export` fails while .envrc is blocked: no cache, retry next time
    mkdir -p "$root/.direnv"
    (cd "$root" && direnv export "$shell" 2>/dev/null) >"$cache.$$" &&
      mv "$cache.$$" "$cache"
    rm -f "$cache.$$"
    [ -e "$cache" ] || return 0
  fi

  # shellcheck source=/dev/null
  . "$cache"
}

_ui_direnv_load
unset -f _ui_direnv_load
