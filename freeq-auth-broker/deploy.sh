#!/bin/bash
# Deploy freeq-auth-broker to Miren
set -e
cd "$(dirname "$0")"

git -C .. rev-parse --short HEAD 2>/dev/null > .git_commit || echo "unknown" > .git_commit

echo "Deploying freeq-auth-broker (commit: $(cat .git_commit))..."
miren deploy -f
