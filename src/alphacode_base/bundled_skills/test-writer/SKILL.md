---
name: test-writer
description: Generate comprehensive test suites: unit tests, integration tests, edge cases, property-based tests, and mutation testing targets.
---

# Test Writer Skill

Generate comprehensive, high-quality test suites.

## Test Pyramid

1. **Unit Tests** (70%) — fast, isolated, test single functions/methods
2. **Integration Tests** (20%) — test component interactions
3. **E2E Tests** (10%) — test full user workflows

## Process

1. **Analyze** — understand the code's public API and edge cases
2. **Plan** — identify test categories (happy path, error, edge, boundary)
3. **Write** — create tests following Arrange-Act-Assert pattern
4. **Verify** — run tests, ensure they pass and are non-flaky
5. **Cover** — check coverage for untested branches

## Test Categories

- **Happy path**: normal expected inputs
- **Error cases**: invalid inputs, missing data, permission denied
- **Edge cases**: empty, null, zero, max values, unicode
- **Boundary**: off-by-one, integer overflow, empty collections
- **Concurrency**: race conditions, deadlocks
- **Integration**: database, network, file system

## Best Practices

- One assertion per test (or one logical assertion)
- Test names describe behavior: `test_withdraw_insufficient_balance_returns_error`
- Use setup/teardown for shared state
- Mock external dependencies, not internal logic
- Tests should be deterministic and independent
- Prefer real objects over mocks when practical
