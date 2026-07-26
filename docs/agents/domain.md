# Domain Docs

## Layout: Single Context

All domain documentation lives under `docs/` at the repository root.

- **Specs**: `docs/superpowers/specs/` — design specs for features
- **Plans**: `docs/superpowers/plans/` — implementation plans
- **Progress**: `docs/*.md` — module progress tracking
- **Reference**: `docs/miva/en.md`, `docs/generics.md` — language reference

## Consumer Rules

When working on a feature, always read:
1. `docs/superpowers/specs/<feature>-design.md` — the spec
2. `docs/superpowers/plans/<feature>.md` — the plan
3. `docs/ERROR_CODES.md` — existing error codes (don't reuse)
4. `docs/miva/en.md` — language reference for syntax validation
