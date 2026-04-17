# JYC Project

JYC is a channel-agnostic AI agent framework written in Rust.
It monitors inbound channels (email via IMAP), routes messages to threads,
and uses OpenCode to generate AI replies.

## Tech Stack
- Rust, tokio async runtime
- IMAP/SMTP for email channels
- OpenCode as the AI backend
- Docker for containerized deployment

## Code Conventions
- Use `tracing` for all logging (never `println!`)
- Error handling: propagate with `?`, use `.context()` for meaningful errors
- All public functions must have doc comments

## Git Rules
- NEVER run `git config user.name` or `git config user.email` (local or global)
- NEVER run `git config --global` for any setting

## Development Workflow
- Always create a feature branch: `git checkout -b feat/<name>`
- After changes, run tests: `cargo test`
- Commit with clear messages describing what changed and why
- Push immediately after committing

## References
- See DESIGN.md for architecture
- See CHANGELOG.md for version history
- See IMPLEMENTATION.md for implementation phases
- OpenCode Server API: https://opencode.ai/docs/server/
