# SquidScript Roadmap

This file is the repository issue tracker for agent-visible project work.
Keep entries concise and actionable. When a roadmap item is completed, remove
it from this file in the same change or in the next cleanup commit.

## Current Track: ESP32-C3 Persistent Reference Runtime

Goal: finish turning the ESP32-C3 Super Mini reference firmware into a
persistent SquidScript app platform prototype.

### 1. Revisit App Manifests

- Decide whether manifests are still needed after SQBC metadata covers app id,
  target, state schema, and chunk metadata.
- Remove or narrow manifest docs/code before 1.0 if they are redundant.
