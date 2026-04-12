#!/bin/bash
# Deploy freeq-site to Miren
# Copies docs from repo root before deploying

set -e
cd "$(dirname "$0")"

# Copy docs from parent repo (these get uploaded with the deploy)
rm -rf docs
cp -r ../docs ./docs

# Write git commit hash for the /version endpoint
git -C .. rev-parse --short HEAD 2>/dev/null > .git_commit || echo "unknown" > .git_commit

echo "Deploying freeq-site (commit: $(cat .git_commit))..."
miren deploy -f

echo "Deployed! Docs will be at https://www.freeq.at/docs/"
