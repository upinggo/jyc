# JYC Docker Deployment

Run JYC as a containerized service with process supervision via s6-overlay.

## Architecture

- **Multi-stage build**: Stage 1 compiles the Rust binary. Stage 2 is a runtime image with dev tools.
- **Single binary**: `jyc` (the MCP reply tool is a hidden subcommand `jyc mcp-reply-tool`).
- **s6-overlay**: Process supervision — automatically restarts JYC if it crashes.
- **Self-bootstrapping**: JYC source is bind-mounted into a thread workspace directory. The AI can rebuild, test, and deploy new versions without rebuilding the Docker image.

## Included Tools

| Tool | Purpose |
|------|---------|
| OpenCode | AI agent runtime |
| git, gh CLI | Version control, PRs |
| ripgrep, jq | Code search, JSON processing |
| curl | HTTP requests |
| build-essential | C compiler for native deps |

**Note:** Rust is NOT pre-installed to keep the image small (~740MB vs ~2GB). The AI installs it on-demand when a rebuild is needed (~30 seconds). See `system.md.example` for the bootstrapping workflow.

## Prerequisites

- Docker or Podman
- JYC data directory with `config.toml` configured
- OpenCode config file (`opencode.jsonc`) with API keys

## Directory Structure

```
/opt/jyc/                    ← JYC data (bind-mounted from host)
├── config.toml              ← JYC configuration
├── .env                     ← Environment variables for secrets
├── <channel>/
│   ├── .imap/               ← IMAP state
│   └── workspace/           ← Thread workspaces
│       └── <bootstrap-thread>/
│           └── jyc/         ← JYC source (bind-mounted for self-bootstrapping)
├── .netrc                   ← Git credentials
└── gh_hosts.yml             ← GitHub CLI auth

/root/.config/opencode/
└── opencode.jsonc           ← OpenCode global config (bind-mounted)

/root/.claude/skills/        ← Skills (bind-mounted, read-only)
/root/.agents/skills/        ← Agent skills (bind-mounted, read-only)
```

## Quick Start

### 1. Create `.env` file

```bash
cp docker/.env.example docker/.env
# Edit docker/.env with your paths
```

### 2. Build and start

**With Docker Compose:**
```bash
cd docker
docker compose up --build -d
docker compose logs -f
```

**With Podman (without Compose):**
```bash
# Build
podman build -t jyc:latest -f docker/Dockerfile .

# Run
podman run -d --name jyc \
  -v /path/to/jyc-data:/opt/jyc \
  -v /path/to/opencode.jsonc:/root/.config/opencode/opencode.jsonc:ro \
  -v /path/to/.claude/skills:/root/.claude/skills:ro \
  -v /path/to/.agents/skills:/root/.agents/skills:ro \
  --restart unless-stopped \
  jyc:latest
```

### 3. Check logs

```bash
docker compose logs -f jyc
# or
podman logs -f jyc
```

### 4. Restart the service

```bash
# From inside the container (e.g., after AI rebuild + deploy):
s6-svc -r /run/service/jyc

# From outside:
docker compose restart jyc
```

## Self-Bootstrapping

The AI (via OpenCode) can rebuild and deploy JYC from inside the container:

1. Copy `docker/system.md.example` to your thread's `system.md`
2. Send an email asking the AI to build and deploy
3. The AI will:
   - Run `cargo test` to verify
   - Run `cargo build --release` to compile
   - Copy the binary to `/usr/local/bin/jyc`
   - Restart the service via `s6-svc -r /run/service/jyc`
4. JYC restarts and sends a startup notification email confirming readiness

## Volume Mounts

| Mount | Container Path | Purpose |
|-------|---------------|---------|
| JYC data dir | `/opt/jyc` | Config, channels, workspace |
| OpenCode config | `/root/.config/opencode/opencode.jsonc` | API keys, providers |
| OpenCode data | `/root/.local/share/opencode` | Sessions DB, logs, snapshots (persisted across restarts) |
| Claude skills | `/root/.claude/skills` | Skills (read-only) |
| Agent skills | `/root/.agents/skills` | Agent skills (read-only) |
| JYC source | `<channel>/workspace/<thread>/jyc` | For AI self-bootstrapping (bind-mounted) |
| .netrc | `/root/.netrc` | Git credentials |
| gh_hosts.yml | `/root/.config/gh/hosts.yml` | GitHub CLI auth |

## Troubleshooting

### Container starts but JYC doesn't run
Check s6 service logs:
```bash
docker exec -it jyc cat /var/log/s6-current/jyc
```

### OpenCode not found
The install script may fail behind a proxy. Build with proxy args:
```bash
docker build --build-arg HTTP_PROXY=http://proxy:8080 --build-arg HTTPS_PROXY=http://proxy:8080 -t jyc:latest -f docker/Dockerfile .
```

### Skills not available
Ensure the skills directories are mounted and readable:
```bash
docker exec -it jyc ls /root/.claude/skills/
docker exec -it jyc ls /root/.agents/skills/
```
