#!/bin/bash
set -e

# Ignore hangup signal — this script may outlive its parent process
# (e.g., when jyc deploys itself via OpenCode).
trap '' HUP

echo "=== JYC Deployment Script ==="

# Source environment variables (for JYC_BINARY)
if [ -f ~/.zshrc.local ]; then
  set -a
  source ~/.zshrc.local
  set +a
fi

# Auto-detect: script is in the jyc repo directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NEW_BINARY="${SCRIPT_DIR}/target/release/jyc"

# Install path from JYC_BINARY env var (set in ~/.zshrc.local)
INSTALL_BINARY="${JYC_BINARY}"

if [ -z "$INSTALL_BINARY" ]; then
    echo "ERROR: JYC_BINARY environment variable not set."
    echo "Add 'export JYC_BINARY=/path/to/jyc' to ~/.zshrc.local"
    exit 1
fi

# Detect whether systemctl --user is available
has_systemd_user() {
    systemctl --user daemon-reload 2>/dev/null
}

# PID file for nohup fallback
PIDFILE="${JYC_WORKDIR:-.}/jyc.pid"
LOGFILE="${JYC_WORKDIR:-.}/jyc.log"

echo "Source binary: ${NEW_BINARY}"
echo "Install target: ${INSTALL_BINARY}"
echo ""

# Check if new binary exists
if [ ! -f "${NEW_BINARY}" ]; then
    echo "ERROR: New binary not found at ${NEW_BINARY}"
    echo "Run 'cargo build --release' first"
    exit 1
fi

echo "New binary found: ${NEW_BINARY}"
NEW_VERSION=$("${NEW_BINARY}" --version 2>/dev/null || echo "unknown")
echo "  Version: ${NEW_VERSION}"
echo ""

# Show version of installed binary (if exists)
if [ -f "${INSTALL_BINARY}" ]; then
    echo "Installed binary: ${INSTALL_BINARY}"
    OLD_VERSION=$("${INSTALL_BINARY}" --version 2>/dev/null || echo "unknown")
    echo "  Version: ${OLD_VERSION}"
    echo ""
fi

# Wait for any pending reply to be delivered before stopping the service.
# deploy.sh is launched via systemd-run (returns immediately), and the AI
# sends a reply right after. This delay ensures the reply reaches the user.
echo "Waiting 5 seconds for pending operations..."
sleep 5

# Stop service
echo "Stopping jyc..."
if has_systemd_user; then
    if systemctl --user is-active --quiet jyc 2>/dev/null; then
        systemctl --user stop jyc
        echo "  Service stopped (systemd)"
    else
        echo "  Service not running (systemd)"
    fi
else
    if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
        kill "$(cat "$PIDFILE")"
        sleep 1
        echo "  Process stopped (nohup, PID $(cat "$PIDFILE"))"
        rm -f "$PIDFILE"
    else
        echo "  Process not running (nohup)"
    fi
fi
echo ""

# Copy new binary
echo "Copying new binary to ${INSTALL_BINARY}..."
cp "${NEW_BINARY}" "${INSTALL_BINARY}"
chmod +x "${INSTALL_BINARY}"
echo "  Binary installed"
echo ""

# Start service
echo "Starting jyc..."
if has_systemd_user; then
    systemctl --user start jyc
    sleep 2
    if systemctl --user is-active --quiet jyc; then
        echo "  Service started successfully (systemd)"
    else
        echo "  Service failed to start"
        echo "  Check logs: journalctl --user -u jyc -n 50"
        exit 1
    fi
    echo ""
    echo "Service status:"
    systemctl --user status jyc --no-pager | head -3
else
    nohup "$SCRIPT_DIR/run-jyc.sh" > "$LOGFILE" 2>&1 &
    echo $! > "$PIDFILE"
    sleep 2
    if kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
        echo "  Process started (nohup, PID $(cat "$PIDFILE"))"
    else
        echo "  Process failed to start"
        echo "  Check logs: $LOGFILE"
        exit 1
    fi
fi
echo ""

echo "=== Deployment complete ==="
