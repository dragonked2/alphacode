---
name: data-pipeline
description: Design and optimize data pipelines: ETL patterns, stream processing, batch jobs, and data validation.
---

# Data Pipeline Skill

Design reliable, observable data pipelines.

## Patterns

- **ETL**: Extract from source → Transform → Load to destination
- **ELT**: Extract → Load raw → Transform in place (data lake pattern)
- **Stream**: Real-time processing with Kafka/Kinesis/Pulsar
- **Batch**: Scheduled processing (daily/hourly)
- **Lambda**: Batch + stream layers combined

## Reliability Checklist

- Idempotent operations (safe to retry)
- Dead letter queues for failed messages
- Checkpointing for restart recovery
- Schema validation at boundaries
- Backpressure handling
- Circuit breakers for downstream failures
- Monitoring: lag, throughput, error rate, data freshness

## Data Validation

- Validate schema at ingestion (reject bad data early)
- Check for nulls, duplicates, out-of-range values
- Monitor data freshness and completeness
- Alert on anomalies (sudden drop in volume, spike in errors)
