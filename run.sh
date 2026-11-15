#!/bin/bash
set -e

# Navigate to project root directory
cd "$(dirname "$0")" || exit 1

exec bash scripts/run.sh "$@"
