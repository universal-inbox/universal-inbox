# Launch an interactive devcontainer shell (e.g. `just dev-shell`, `just dev-shell claude`)
# Add --firewall for network-firewalled autonomous mode.
[positional-arguments]
dev-shell *args:
    #!/usr/bin/env bash
    set -euo pipefail
    docker build -t rmcp-actix-web-devcontainer .devcontainer/
    tty_flag=$( [[ -t 0 ]] && echo "-it" || echo "-i" )
    run_args=(
        --rm $tty_flag --init
        -v "$(pwd):$(pwd)" -w "$(pwd)"
        -v "$SSH_AUTH_SOCK:/tmp/ssh-agent.sock"
        -e SSH_AUTH_SOCK=/tmp/ssh-agent.sock
        -e COLORTERM="${COLORTERM:-}"
        -e DEVCONTAINER_WORKSPACE="$(pwd)"
    )
    # Firewalled mode: iptables egress filter + run as root then drop privileges via gosu
    # Normal mode: run directly as host UID (no firewall, no caps)
    if [[ "${1:-}" = "--firewall" ]]; then
        shift
        run_args+=(
            --cap-add=NET_ADMIN --cap-add=NET_RAW
            -e DEVCONTAINER_FIREWALL=1
            -e DEVCONTAINER_UID="$(id -u)"
            -e DEVCONTAINER_GID="$(id -g)"
        )
    else
        run_args+=(--user "$(id -u):$(id -g)")
    fi
    # Conditional host config mounts
    [[ -f "$HOME/.gitconfig" ]] && run_args+=(-v "$HOME/.gitconfig:/tmp/home/.gitconfig:ro")
    [[ -d "$HOME/.config/glab-cli" ]] && run_args+=(-v "$HOME/.config/glab-cli:/tmp/glab-config")
    [[ -d "$HOME/.claude" ]] && run_args+=(
        -v "$HOME/.claude:/tmp/home/.claude"
        -v "$HOME/.claude:$HOME/.claude"
    )
    [[ -f "$HOME/.claude.json" ]] && run_args+=(-v "$HOME/.claude.json:/tmp/home/.claude.json")
    if [[ $# -eq 0 ]]; then
        exec docker run "${run_args[@]}" rmcp-actix-web-devcontainer bash
    else
        exec docker run "${run_args[@]}" rmcp-actix-web-devcontainer "$@"
    fi
