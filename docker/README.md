# JYC Docker Deployment

Run JYC as a containerized service with process supervision via s6-overlay.

## Architecture

- **Multi-stage build**: `base` (shared tools) → `builder` (Rust compile) → `production`
- **Single binary**: `jyc` (the MCP reply tool is a hidden subcommand `jyc mcp-reply-tool`).
- **s6-overlay**: Process supervision — automatically restarts JYC if it crashes.
- **Host networking**: Container shares host network (`network_mode: host`), so services on `localhost` are accessible from inside the container.

## Included Tools

| Tool | Purpose |
|------|---------|
| OpenCode | AI agent runtime |
| git, gh CLI | Version control, GitHub PRs |
| ripgrep, jq | Code search, JSON processing |
| curl | HTTP requests |
| build-essential | C compiler for native deps |
| python3 | Python scripting |
| nodejs (LTS) | JavaScript/TypeScript runtime |
| pandoc | Document conversion |

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

**With Docker/Podman Compose:**
```bash
cd docker
docker compose up --build -d
docker compose logs -f
```

**With Podman (without Compose):**
```bash
# Build
podman build -t jyc:latest -f docker/Dockerfile ..

# Run
podman run -d --name jyc \
  --network=host \
  -v /path/to/jyc-data:/opt/jyc \
  -v /path/to/opencode.jsonc:/root/.config/opencode/opencode.jsonc:ro \
  -v /path/to/.claude/skills:/root/.claude/skills:ro \
  -v /path/to/.agents/skills:/root/.agents/skills:ro \
  -v /path/to/jyc-data/.netrc:/root/.netrc \
  -v /path/to/jyc-data/gh_hosts.yml:/root/.config/gh/hosts.yml \
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
# From inside the container:
s6-svc -r /run/service/jyc

# From outside:
docker compose restart jyc
```

## Volume Mounts

| Mount | Container Path | Purpose |
|-------|---------------|---------|
| JYC data dir | `/opt/jyc` | Config, channels, workspace |
| OpenCode config | `/root/.config/opencode/opencode.jsonc` | API keys, providers |
| OpenCode data | `/root/.local/share/opencode` | Sessions DB, logs, snapshots |
| Claude skills | `/root/.claude/skills` | Skills (read-only) |
| Agent skills | `/root/.agents/skills` | Agent skills (read-only) |
| .netrc | `/root/.netrc` | Git credentials |
| gh_hosts.yml | `/root/.config/gh/hosts.yml` | GitHub CLI auth |

### Project Source Mounts

To give the AI agent access to project source code, mount directories into
the thread workspace. Customize per your channel/thread layout:

```yaml
volumes:
  - /path/to/project:/opt/jyc/<channel>/workspace/<thread>/project
```

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
