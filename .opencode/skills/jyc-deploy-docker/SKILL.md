---
name: jyc-deploy-docker
description: Build and deploy jyc in Docker container. Use when instructed to build, deploy, or build-and-deploy jyc.
---

IMPORTANT: The JYC source repository is at ./jyc/ relative to your working directory.
All build commands below MUST be run from the jyc/ directory (use `cd jyc` first).

## Critical Safety: Build/Deploy Operations
CRITICAL: AI MUST use TWO-PHASE CONFIRMATION for ALL build/deploy operations:

Phase 1 - Present Plan:
- List ALL commands that will be executed
- Explain what each command does
- State potential risks (e.g., "build will take 2-3 min", "deploy will restart the container")

Phase 2 - Wait for Approval:
- Ask the user to type "yes" or "proceed" to continue
- If the user responds with anything else, ABORT the operation
- Only execute commands after receiving explicit "yes" or "proceed"

CRITICAL: Never execute build/deploy commands without explicit user approval.

## Docker Deployment

The Docker image is a production-only image. There is no Rust toolchain inside
the container. To update jyc, rebuild the Docker image from the host.

### Build (when instructed)
MUST follow TWO-PHASE CONFIRMATION above before executing.
MUST execute EXACTLY these commands in order:

1. cd jyc && cargo test
2. cargo build --release
3. ./target/release/jyc --version
4. Report: version, binary size, test results

### Deploy (when instructed)
MUST follow TWO-PHASE CONFIRMATION above before executing.

WARNING: Deploy rebuilds the container image and restarts the service.

MUST execute EXACTLY these steps in order:

Step 1: Rebuild the Docker image:
   ```
   cd jyc
   podman build -t jyc:latest -f docker/Dockerfile .
   ```
Step 2: Restart the container:
   ```
   cd jyc/docker
   podman compose down && podman compose up -d
   ```
Step 3: Verify the service is running:
   ```
   podman compose logs -f jyc
   ```

### Build and Deploy (when instructed)
MUST follow TWO-PHASE CONFIRMATION above before executing.
MUST execute EXACTLY these steps in order:

1. Run the Build steps above (test + compile locally to verify)
2. Run the Deploy steps above (rebuild image + restart container)
