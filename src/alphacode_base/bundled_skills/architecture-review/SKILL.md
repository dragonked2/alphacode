---
name: architecture-review
description: Review system architecture: module boundaries, dependency direction, coupling analysis, and scalability assessment.
---

# Architecture Review Skill

Evaluate system architecture for quality and sustainability.

## Process

1. **Map** — draw the dependency graph between modules/packages
2. **Analyze** — check dependency direction (should flow inward to core)
3. **Coupling** — identify tight coupling, circular dependencies
4. **Boundaries** — verify module responsibilities are clear and single-purpose
5. **Scale** — assess what breaks under 10x/100x load
6. **Report** — findings with risk level and recommendations

## Quality Attributes

- **Modularity**: clear boundaries, low coupling, high cohesion
- **Testability**: can modules be tested in isolation
- **Extensibility**: can new features be added without modifying existing code
- **Scalability**: horizontal scaling path is clear
- **Maintainability**: changes are localized, ripple effects are minimal
- **Observability**: system state is visible through logs, metrics, traces

## Anti-Patterns

- God modules that know about everything
- Circular dependencies between packages
- Leaky abstractions (internal details exposed across boundaries)
- Shared mutable state between modules
- Configuration scattered across multiple locations
