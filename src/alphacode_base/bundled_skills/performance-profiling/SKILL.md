---
name: performance-profiling
description: Identify performance bottlenecks: CPU profiling, memory analysis, I/O optimization, and benchmarking strategies.
---

# Performance Profiling Skill

Systematic performance investigation and optimization.

## Process

1. **Define target** — what metric to improve (latency, throughput, memory)
2. **Baseline** — measure current performance under realistic conditions
3. **Profile** — use appropriate profiler for the bottleneck type
4. **Identify** — find the top hotspots (usually 20% of code causes 80% of time)
5. **Optimize** — targeted fix for the biggest bottleneck
6. **Verify** — re-measure, confirm improvement, check for regressions

## Profiling Tools by Bottleneck

- **CPU**: flame graphs, sampling profilers, perf/Instruments
- **Memory**: heap dumps, allocation trackers, leak detectors
- **I/O**: strace, network traces, disk benchmarks
- **Concurrency**: thread analyzers, lock contention profiling

## Benchmarking Rules

- Warm up before measuring (JIT, caches, connection pools)
- Run enough iterations for statistical significance
- Measure in production-like environment
- Isolate from noise (dedicated machine, no other workloads)
- Use percentiles (p50, p95, p99), not just averages
