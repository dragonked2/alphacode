---
name: optimization
description: Improve performance, latency, throughput, memory usage, or general efficiency by defining metrics, measuring, attributing bottlenecks, and prioritizing macro-optimizations.
---

# Optimization Skill

Systematic performance optimization workflow.

## Process

1. **Define Metrics** — identify what to measure (latency p50/p99, throughput, memory, CPU)
2. **Measure Baseline** — capture current performance before any changes
3. **Profile** — use profiling tools to find actual bottlenecks (don't guess)
4. **Attribute** — map bottlenecks to specific code paths
5. **Prioritize** — fix the highest-impact bottleneck first
6. **Optimize** — apply targeted changes (algorithm, data structure, caching, concurrency, I/O)
7. **Verify** — re-measure and confirm improvement

## Rules

- Never optimize without a baseline measurement
- Never guess at bottlenecks — profile first
- One change at a time so impact is attributable
- Document before/after metrics for every optimization
- Prefer algorithmic improvements over micro-optimizations
- If optimization reduces readability, add a comment explaining why

## Common Patterns

- **Algorithmic**: O(n^2) → O(n log n) or O(n)
- **Caching**: memoize expensive repeated computations
- **Batching**: combine many small operations into fewer large ones
- **Lazy evaluation**: defer work until result is actually needed
- **Concurrency**: parallelize independent work
- **I/O**: async, buffering,减少 system calls
- **Memory**: reduce allocations, reuse buffers, arena allocators
