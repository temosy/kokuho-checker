#!/usr/bin/env bash
# Sync sources to Fedora so the temosy-wordpress compose can build the
# kokuho image from ../kokuho-checker (same relative layout as on the Mac).
set -euo pipefail
cd "$(dirname "$0")/.."
ssh haruo@192.168.1.18 "mkdir -p kakari-deploy/kokuho-checker"
rsync -az --delete --exclude .git --exclude target ./ haruo@192.168.1.18:kakari-deploy/kokuho-checker/
echo "✓ 同期完了。次: temosy-wordpress 側で sudo podman compose up -d --build kokuho nginx"
