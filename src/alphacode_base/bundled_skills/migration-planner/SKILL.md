---
name: migration-planner
description: Plan and execute migrations: database schema changes, API versioning, breaking changes, and rollback procedures.
---

# Migration Planner Skill

Safe, reversible migration planning.

## Process

1. **Assess** — scope of change, affected systems, data volume
2. **Plan** — step-by-step migration with rollback at each step
3. **Test** — dry-run against production-like data
4. **Execute** — run migration with monitoring
5. **Verify** — validate data integrity post-migration
6. **Clean up** — remove old code/tables after bake period

## Database Migration Rules

- Never delete columns/tables in the same deploy as code changes
- Use expand-contract pattern: add new → migrate data → remove old
- Backward-compatible schema changes (additive only)
- Test migrations against a copy of production data
- Keep rollback scripts for every migration
- Use transactions for atomic multi-step migrations

## API Versioning

- URL path versioning: `/v1/`, `/v2/` (simplest)
- Deprecation headers: `Sunset`, `Deprecation`
- Support old version for minimum 6 months after new version
- Document breaking changes in changelog
