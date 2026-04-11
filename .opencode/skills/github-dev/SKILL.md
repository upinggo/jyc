---
name: github-dev
description: |
  GitHub issue/PR development workflow. Use when working on GitHub issues,
  creating branches, implementing fixes, creating PRs, or responding to
  PR review comments.
---

## GitHub Issue/PR Development Workflow

### Working on a GitHub Issue

1. Read the issue carefully and understand the requirement
2. Read DESIGN.md and relevant source code for context
3. Create a feature branch:
   ```bash
   git checkout -b fix/issue-<number>   # for bugs
   git checkout -b feat/issue-<number>  # for features
   ```
4. Implement the fix/feature following the incremental-dev approach
5. Run tests: `cargo test`
6. Build clean: `cargo build --release` (zero warnings)
7. Commit with clear message referencing the issue:
   ```
   fix: description of the fix (#<issue_number>)
   ```

### Creating a PR

When creating a PR for a GitHub issue, ALWAYS:

1. Include `Fixes #<issue_number>` in the PR body
   - This links the PR to the issue
   - Comments on the PR are routed to the same thread as the issue
   - The issue is automatically closed when the PR is merged

2. Use `gh pr create`:
   ```bash
   gh pr create --title "fix: description" --body "$(cat <<'EOF'
   ## Summary
   <description>

   Fixes #<issue_number>
   EOF
   )"
   ```

3. After creating the PR, report the PR number and URL in your reply

### Responding to PR Review Comments

When review comments are received on the PR:

1. Read the review comments carefully
2. Fix the issues on the same branch
3. Commit and push: `git add . && git commit -m "fix: address review feedback" && git push`
4. Reply with what was fixed

### Branch Naming

- Bug fixes: `fix/issue-<number>` (e.g., `fix/issue-42`)
- Features: `feat/issue-<number>` (e.g., `feat/issue-42`)
- If the issue title is descriptive: `fix/<short-description>` (e.g., `fix/imap-timeout`)

### Commit Messages

Reference the issue number in commits:
- `fix: resolve IMAP timeout (#42)`
- `feat: add vision tool support (#42)`

### Rules

- ALWAYS create a feature branch — never commit directly to main
- ALWAYS include `Fixes #<issue_number>` in PR body
- ALWAYS run tests before creating a PR
- ALWAYS reply with the PR URL after creation
- Use `gh` CLI for PR operations (already authenticated)
