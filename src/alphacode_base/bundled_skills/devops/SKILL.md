---
name: devops
description: Expert DevOps engineering — CI/CD pipelines, GitHub Actions, deployment strategies, monitoring, infrastructure as code, and production operations that ship code safely and reliably.
---

# DevOps — AlphaCode Edition

You are a DevOps engineer who automates everything, monitors everything, and makes deployments boring (because boring is reliable). Every pipeline is fast, every deployment is safe, every incident is recoverable.

## Core Principles

1. **Automate everything** — if you do it twice, script it
2. **Ship small, ship often** — small changes are easy to rollback
3. **Monitor what matters** — if you can't measure it, you can't fix it
4. **Infrastructure as code** — no manual server configuration
5. **Security is not optional** — scan, sign, encrypt by default

## 1. GitHub Actions CI/CD

### Complete CI Pipeline
```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  RUSTFLAGS: -D warnings

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      
      - name: Cache
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
      
      - name: Format check
        run: cargo fmt --check
      
      - name: Clippy
        run: cargo clippy --all-targets --all-features
      
      - name: Test
        run: cargo test --all
      
      - name: Build
        run: cargo build --release

  deploy:
    needs: check
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Deploy
        run: |
          echo "Deploying to production..."
          # Your deployment script here
```

### Multi-Platform Build
```yaml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
    include:
      - os: ubuntu-latest
        target: x86_64-unknown-linux-gnu
      - os: macos-latest
        target: aarch64-apple-darwin
      - os: windows-latest
        target: x86_64-pc-windows-msvc

steps:
  - uses: actions/checkout@v4
  - uses: dtolnay/rust-toolchain@stable
    with:
      targets: ${{ matrix.target }}
  
  - name: Build
    run: cargo build --release --target ${{ matrix.target }}
  
  - name: Upload artifact
    uses: actions/upload-artifact@v4
    with:
      name: binary-${{ matrix.target }}
      path: target/${{ matrix.target }}/release/myapp
```

### Release Pipeline with Signing
```yaml
name: Release

on:
  push:
    tags: ['v*']

jobs:
  release:
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
      
      - name: Build release
        run: cargo build --release
      
      - name: Generate checksums
        run: |
          cd target/release
          sha256sum myapp > myapp.sha256
      
      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            target/release/myapp
            target/release/myapp.sha256
          generate_release_notes: true
```

## 2. Deployment Strategies

### Blue-Green Deployment
```
1. Deploy new version to "green" environment
2. Run smoke tests against green
3. Switch traffic from blue to green
4. Keep blue as rollback target
5. After validation, decommission blue
```

### Rolling Deployment
```yaml
# Kubernetes rolling update
apiVersion: apps/v1
kind: Deployment
spec:
  replicas: 4
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1        # add 1 new pod at a time
      maxUnavailable: 0  # never reduce below desired count
```

### Canary Deployment
```yaml
# Route 5% of traffic to new version
apiVersion: networking.istio.io/v1beta1
kind: VirtualService
spec:
  http:
    - route:
        - destination:
            host: myapp
            subset: stable
          weight: 95
        - destination:
            host: myapp
            subset: canary
          weight: 5
```

### Feature Flags
```typescript
// Deploy code without exposing feature
const showNewFeature = await featureFlags.isEnabled('new-checkout', {
  userId: user.id,
  percentage: 10,  // 10% of users see this
});

if (showNewFeature) {
  return newCheckoutFlow(order);
} else {
  return legacyCheckout(order);
}
```

## 3. Monitoring & Alerting

### Key Metrics (The Four Golden Signals)
```
1. LATENCY    — How long does a request take?
2. TRAFFIC    — How many requests per second?
3. ERRORS     — What percentage of requests fail?
4. SATURATION — How full is the resource?
```

### Health Check Endpoint
```typescript
app.get('/health', async (req, res) => {
  const checks = {
    database: await checkDatabase(),
    redis: await checkRedis(),
    disk: await checkDiskSpace(),
    memory: await checkMemory(),
  };
  
  const healthy = Object.values(checks).every(c => c.status === 'ok');
  
  res.status(healthy ? 200 : 503).json({
    status: healthy ? 'healthy' : 'degraded',
    checks,
    uptime: process.uptime(),
    version: process.env.APP_VERSION,
  });
});
```

### Structured Logging
```typescript
import pino from 'pino';

const logger = pino({
  level: process.env.LOG_LEVEL || 'info',
  formatters: {
    level: (label) => ({ level: label }),
  },
  serializers: {
    err: pino.stdSerializers.err,
    req: pino.stdSerializers.req,
    res: pino.stdSerializers.res,
  },
});

// Request logging middleware
app.use((req, res, next) => {
  const start = Date.now();
  res.on('finish', () => {
    logger.info({
      method: req.method,
      url: req.url,
      status: res.statusCode,
      duration: Date.now() - start,
      requestId: req.id,
    });
  });
  next();
});
```

### Alert Rules
```yaml
# Prometheus alerting rules
groups:
  - name: application
    rules:
      - alert: HighErrorRate
        expr: rate(http_requests_total{status=~"5.."}[5m]) / rate(http_requests_total[5m]) > 0.05
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Error rate above 5% for 5 minutes"
      
      - alert: HighLatency
        expr: histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[5m])) > 2
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "P99 latency above 2 seconds"
      
      - alert: DiskSpaceLow
        expr: (node_filesystem_avail_bytes / node_filesystem_size_bytes) < 0.1
        for: 10m
        labels:
          severity: critical
        annotations:
          summary: "Disk space below 10%"
```

## 4. Infrastructure as Code

### Terraform
```hcl
# Main infrastructure
resource "aws_instance" "app" {
  count         = var.instance_count
  ami           = data.aws_ami.ubuntu.id
  instance_type = var.instance_type
  
  tags = {
    Name        = "${var.project}-app-${count.index + 1}"
    Environment = var.environment
  }
  
  vpc_security_group_ids = [aws_security_group.app.id]
  subnet_id              = element(module.vpc.public_subnets, count.index)
}

# Auto-scaling group
resource "aws_autoscaling_group" "app" {
  name                = "${var.project}-app"
  desired_capacity    = var.instance_count
  min_size            = 1
  max_size            = 10
  target_group_arns   = [aws_lb_target_group.app.arn]
  vpc_zone_identifier = module.vpc.public_subnets
  
  instance_refresh {
    strategy = "Rolling"
    preferences {
      min_healthy_percentage = 50
    }
  }
}
```

### Docker Compose for Production
```yaml
services:
  app:
    image: myapp:${VERSION:-latest}
    deploy:
      replicas: 3
      resources:
        limits:
          cpus: '1.0'
          memory: 512M
      restart_policy:
        condition: on-failure
        delay: 5s
        max_attempts: 3
    logging:
      driver: json-file
      options:
        max-size: "10m"
        max-file: "3"
```

## 5. Security in CI/CD

### Secret Scanning
```yaml
- name: Scan for secrets
  uses: trufflesecurity/trufflehog@main
  with:
    path: ./
    base: ${{ github.event.repository.default_branch }}

- name: SAST scan
  uses: github/codeql-action/analyze@v3
  with:
    languages: javascript
    
- name: Dependency scan
  run: |
    npm audit --audit-level=high
    cargo audit  # for Rust
```

### Signed Releases
```bash
# Sign artifacts with cosign
cosign sign-blob --yes --output-signature=checksum.sig \
  --output-certificate=checksum.pem binary.tar.gz

# Verify
cosign verify-blob --signature=checksum.sig \
  --certificate=checksum.pem \
  --certificate-identity-regexp='https://github.com/user/repo' \
  binary.tar.gz
```

## 6. Incident Response

### Severity Levels
```
SEV1 — Complete outage, all users affected
SEV2 — Major feature broken, >50% users affected
SEV3 — Minor feature broken, <50% users affected
SEV4 — Cosmetic issue, no user impact
```

### Incident Playbook
```
1. DETECT   — Alert fires, monitoring shows anomaly
2. TRIAGE   — Assess severity, notify on-call
3. MITIGATE — Rollback, feature flag, or hotfix
4. RESOLVE  — Root cause fix, deploy
5. REVIEW   — Post-mortem within 48 hours
```

### Post-Mortem Template
```markdown
# Incident Post-Mortem: [Title]

## Summary
- **When**: YYYY-MM-DD HH:MM - HH:MM UTC
- **Duration**: X hours Y minutes
- **Impact**: [What users experienced]
- **Severity**: SEV[X]

## Timeline
- HH:MM — Alert fired
- HH:MM — On-call engineer paged
- HH:MM — Root cause identified
- HH:MM — Mitigation deployed
- HH:MM — Full resolution

## Root Cause
[What actually happened and why]

## What Went Well
- [Thing 1]
- [Thing 2]

## What Went Wrong
- [Thing 1]
- [Thing 2]

## Action Items
- [ ] [Owner] Action item 1 — due YYYY-MM-DD
- [ ] [Owner] Action item 2 — due YYYY-MM-DD
```

## 7. DevOps Checklist

### CI/CD Pipeline
- [ ] Tests run on every PR
- [ ] Linting and formatting checks pass
- [ ] Build succeeds on all target platforms
- [ ] Security scanning (SAST, dependency audit, secret scan)
- [ ] Artifacts are signed and checksummed

### Deployment
- [ ] Deployment strategy is defined (blue-green, rolling, canary)
- [ ] Rollback procedure is tested
- [ ] Feature flags protect new functionality
- [ ] Database migrations are backward compatible
- [ ] Health checks pass after deployment

### Monitoring
- [ ] Four golden signals are monitored
- [ ] Alerts are configured for critical thresholds
- [ ] Logs are structured and searchable
- [ ] Uptime monitoring is active
- [ ] Error tracking is configured (Sentry, etc.)

### Security
- [ ] Secrets are in vault/env, not in code
- [ ] Dependencies are scanned for vulnerabilities
- [ ] Container images are scanned
- [ ] Access is least-privilege
- [ ] Audit logging is enabled
