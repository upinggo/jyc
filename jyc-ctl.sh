#!/usr/bin/bash

# Source environment variables (for JYC_WORKDIR)
if [ -f ~/.zshrc.local ]; then
  set -a
  source ~/.zshrc.local
  set +a
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PIDFILE="${JYC_WORKDIR:-.}/jyc.pid"
LOGFILE="${JYC_WORKDIR:-.}/jyc.log"

# Detect whether systemctl --user is available
has_systemd_user() {
    systemctl --user daemon-reload 2>/dev/null
}

# --- nohup helpers ---

nohup_status() {
    if [ -f "$PIDFILE" ]; then
        local pid
        pid=$(cat "$PIDFILE")
        if kill -0 "$pid" 2>/dev/null; then
            echo "jyc is running (PID $pid)"
            echo "Log: $LOGFILE"
        else
            echo "jyc is not running (stale PID $pid)"
            rm -f "$PIDFILE"
        fi
    else
        echo "jyc is not running (no PID file)"
    fi
}

nohup_stop() {
    if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
        local pid
        pid=$(cat "$PIDFILE")
        kill "$pid"
        sleep 1
        echo "jyc stopped (PID $pid)"
        rm -f "$PIDFILE"
    else
        echo "jyc is not running"
    fi
}

nohup_start() {
    if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
        echo "jyc is already running (PID $(cat "$PIDFILE"))"
        return
    fi
    nohup "$SCRIPT_DIR/run-jyc.sh" > "$LOGFILE" 2>&1 &
    echo $! > "$PIDFILE"
    sleep 1
    if kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
        echo "jyc started (PID $(cat "$PIDFILE"))"
    else
        echo "ERROR: jyc failed to start. Check $LOGFILE"
        rm -f "$PIDFILE"
    fi
}

nohup_logs() {
    if [ -f "$LOGFILE" ]; then
        tail -n 100 -f "$LOGFILE"
    else
        echo "No log file found at $LOGFILE"
    fi
}

# --- main ---

case "$1" in
  status)
    if has_systemd_user; then
        systemctl --user status jyc
    else
        nohup_status
    fi
    ;;
  logs)
    if has_systemd_user; then
        journalctl --user -u jyc -n 100 -f
    else
        nohup_logs
    fi
    ;;
  restart)
    if has_systemd_user; then
        systemctl --user restart jyc
    else
        nohup_stop
        nohup_start
    fi
    ;;
  stop)
    if has_systemd_user; then
        systemctl --user stop jyc
    else
        nohup_stop
    fi
    ;;
  start)
    if has_systemd_user; then
        systemctl --user start jyc
    else
        nohup_start
    fi
    ;;
  *)
    echo "Usage: $0 {status|logs|restart|stop|start}"
    echo ""
    echo "Commands:"
    echo "  status   - Show service status"
    echo "  logs     - Follow service logs"
    echo "  restart  - Restart service"
    echo "  stop     - Stop service"
    echo "  start    - Start service"
    echo ""
    echo "Automatically detects systemd --user vs nohup fallback."
    exit 1
    ;;
esac
