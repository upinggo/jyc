# ── Stage 1: Base tools (cached — rarely changes) ─────────────────────
FROM debian:bookworm-slim AS base

# Dev tools (needed by OpenCode AI for coding tasks)
RUN apt-get update && apt-get install -y --no-install-recommends \
    git curl ripgrep jq procps xz-utils inotify-tools \
    ca-certificates pkg-config libssl-dev build-essential \
    && rm -rf /var/lib/apt/lists/*

# s6-overlay for process supervision
ARG S6_OVERLAY_VERSION=3.1.6.2
RUN curl -fsSL "https://github.com/just-containers/s6-overlay/releases/download/v${S6_OVERLAY_VERSION}/s6-overlay-noarch.tar.xz" -o /tmp/s6-overlay-noarch.tar.xz && \
    tar -C / -Jxpf /tmp/s6-overlay-noarch.tar.xz && rm /tmp/s6-overlay-noarch.tar.xz
RUN ARCH=$(uname -m) && \
    case "$ARCH" in \
      x86_64) S6_ARCH=x86_64 ;; \
      aarch64|arm64) S6_ARCH=aarch64 ;; \
      *) echo "Unsupported architecture: $ARCH" && exit 1 ;; \
    esac && \
    curl -fsSL "https://github.com/just-containers/s6-overlay/releases/download/v${S6_OVERLAY_VERSION}/s6-overlay-${S6_ARCH}.tar.xz" -o /tmp/s6-overlay-arch.tar.xz && \
    tar -C / -Jxpf /tmp/s6-overlay-arch.tar.xz && rm /tmp/s6-overlay-arch.tar.xz

# Install OpenCode
RUN curl -fsSL https://opencode.ai/install | bash \
    && ln -sf /root/.opencode/bin/opencode /usr/local/bin/opencode

# Install GitHub CLI
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
    | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
    && chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
    | tee /etc/apt/sources.list.d/github-cli.list > /dev/null \
    && apt-get update && apt-get install -y --no-install-recommends gh && rm -rf /var/lib/apt/lists/*

# ── Stage 2: Build jyc (changes when source changes) ─────────────────
FROM rust:bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY config_template.toml ./

RUN cargo build --release

# ── Stage 3: Final image (base tools + jyc binary) ───────────────────
FROM base

# Separate target dir for container builds (avoids conflict with host macOS builds
# when source is bind-mounted). Set globally so cargo picks it up after on-demand install.
ENV CARGO_TARGET_DIR="/tmp/jyc-target"

# Copy compiled jyc binary (single binary — reply tool is a hidden subcommand)
COPY --from=builder /build/target/release/jyc /usr/local/bin/jyc

# Note: JYC source for self-bootstrapping is bind-mounted into the thread workspace
# at runtime (via docker-compose), NOT baked into the image.
# Note: Rust is NOT pre-installed to keep the image small (~740MB vs ~2GB).
# The AI installs it on-demand for self-bootstrapping. See system.md.example.

# s6 service definitions
COPY docker/s6-rc.d/ /etc/s6-overlay/s6-rc.d/
RUN touch /etc/s6-overlay/s6-rc.d/user/contents.d/jyc

# Prepare directories for skills, config, and data mounts
RUN mkdir -p /root/.claude/skills /root/.agents/skills \
    /root/.config/opencode /root/.config/gh \
    /root/.local/share/opencode \
    /opt/jyc

# Working directory for jyc data (config.toml, channels, workspace)
WORKDIR /opt/jyc

# Volumes
VOLUME ["/opt/jyc", "/root/.config/opencode", "/root/.claude/skills", "/root/.agents/skills", "/root/.netrc", "/root/.config/gh/hosts.yml"]

ENTRYPOINT ["/init"]
