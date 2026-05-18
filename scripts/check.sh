#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$ROOT"
cargo test

cd "$ROOT/simulator/browser"
npm test
npm run build
npm run test:e2e

