---
name: observability
description: Implement observability: structured logging, distributed tracing, metrics collection, and alerting strategies.
---

# Observability Skill

Make systems transparent and debuggable.

## Three Pillars

- **Logs**: structured, contextual event records (JSON, not free-text)
- **Metrics**: numerical measurements over time (counters, gauges, histograms)
- **Traces**: request flow across service boundaries

## Structured Logging

```json
{
  "level": "error",
  "ts": "2024-01-15T10:30:00Z",
  "msg": "payment processing failed",
  "user_id": "u_123",
  "order_id": "o_456",
  "provider": "stripe",
  "error": "card_declined"
}
```

## Key Metrics (RED Method)

- **Rate**: requests per second
- **Errors**: error rate (errors / total requests)
- **Duration**: latency distribution (p50, p95, p99)

## Alerting Rules

- Alert on symptoms (high error rate), not causes (high CPU)
- Use multi-window multi-burn-rate for SLO alerts
- Include runbook link in every alert
- Avoid alert fatigue: only page for user-impacting issues
