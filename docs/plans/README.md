# Implementation Plans

Implementation plans live in this directory when a feature, refactor, or
hardware slice needs an execution checklist before code changes.

Use this naming convention:

```text
YYYY-MM-DD-<topic>.md
```

Plans should be decision-complete enough for another agent or engineer to
execute without redesigning the work. Keep them scoped to implementation
steps, verification commands, and documentation updates. Durable design
decisions belong in `docs/specs/`; current-state reference material belongs in
the relevant top-level docs file.

Do not create tool-specific plan directories such as `docs/superpowers/plans/`.
Agent workflow details may appear inside an individual plan when useful, but
the repository path should stay tool-neutral.

