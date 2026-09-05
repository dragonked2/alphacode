---
name: debug
description: Expert debugging methodology — systematic error diagnosis, root cause analysis, logging strategies, debugging tools, and problem-solving techniques that find bugs fast instead of guessing.
---

# Debug — AlphaCode Edition

You are a detective investigating a crime scene. The bug is the crime. Your job is to find evidence, follow leads, and identify the root cause — not guess, not blame, not spray-and-pray fixes.

## Core Principles

1. **Reproduce first** — if you can't reproduce it, you can't fix it
2. **Read the error message** — the answer is usually right there
3. **Change one thing at a time** — scientific method, not shotgun debugging
4. **Use tools, not intuition** — profilers, debuggers, logs, traces
5. **Fix the root cause, not the symptom** — band-aids create more bugs

## 1. The Debugging Methodology

### Step 1: Understand the Bug
```
WHAT:    What exactly happens? (error message, wrong behavior, crash)
WHEN:    When does it happen? (specific input, time, condition)
WHERE:   Where does it happen? (file, function, line)
HOW:     How do you reproduce it? (steps to trigger)
SCOPE:   How many people affected? (all users, some, one)
```

### Step 2: Gather Evidence
```bash
# Check logs
tail -100 /var/log/app/error.log
docker logs container_name --tail 100

# Check recent changes
git log --oneline -10
git diff HEAD~3

# Check system state
free -h          # memory
df -h            # disk
top              # CPU
netstat -tlnp    # ports
```

### Step 3: Form a Hypothesis
- What could cause this specific behavior?
- What changed recently that could trigger it?
- What conditions must be true for this to happen?

### Step 4: Test the Hypothesis
- Add logging/print statements at suspected locations
- Use debugger to step through code
- Change one variable and observe the result

### Step 5: Fix and Verify
- Apply the minimal fix that addresses the root cause
- Verify the fix works
- Verify no regressions
- Add a test to prevent recurrence

## 2. Common Error Patterns

### Null/Undefined Reference
```
TypeError: Cannot read property 'x' of undefined
```
**Diagnosis**: Something that should exist doesn't.
**Fix chain**:
1. Check where the variable comes from
2. Check if the data source returns the expected shape
3. Add null checks or default values
4. Fix the upstream issue that returns null

### Connection Errors
```
ECONNREFUSED 127.0.0.1:5432
connect EHOSTUNREACH 10.0.0.1:3306
```
**Diagnosis**: Can't reach the service.
**Check list**:
1. Is the service running? `systemctl status <service>` or `docker ps`
2. Is the port correct? `netstat -tlnp | grep <port>`
3. Is the hostname/IP correct? `ping <host>`
4. Is there a firewall? `iptables -L` or `ufw status`
5. Is the service listening on the right interface? `0.0.0.0` vs `127.0.0.1`

### Timeout Errors
```
ETIMEDOUT: Connection timed out after 30000ms
Operation timed out after 5000ms
```
**Diagnosis**: Request took too long.
**Check list**:
1. Is the server overloaded? Check CPU, memory, connections
2. Is the query slow? `EXPLAIN ANALYZE` the database query
3. Is there network latency? `ping` and `traceroute`
4. Is the timeout too short? Increase and test
5. Is there a deadlock? Check database locks

### Memory Errors
```
JavaScript heap out of memory
MemoryError: Unable to allocate memory
```
**Diagnosis**: Ran out of RAM.
**Check list**:
1. Is there a memory leak? Check if memory grows over time
2. Is the dataset too large? Paginate or stream
3. Are large objects retained in memory? Use weak references
4. Is there a circular reference preventing garbage collection?

## 3. Language-Specific Debugging

### Rust
```bash
# Compile with debug info
cargo build

# Run with RUST_BACKTRACE
RUST_BACKTRACE=1 cargo run

# Run with debug assertions
cargo test

# Use rust-gdb
rust-gdb target/debug/myapp
(gdb) run
(gdb) backtrace
(gdb) info locals
```

### JavaScript/TypeScript
```javascript
// Console debugging
console.log('variable:', variable);
console.table(arrayOfObjects);
console.time('operation');
// ... code ...
console.timeEnd('operation');

// Node.js inspector
node --inspect app.js
# Then open chrome://inspect in Chrome

// Chrome DevTools
// Sources tab → Breakpoints → Step through code
// Profiler tab → Record → Find hot functions
// Memory tab → Take heap snapshot → Compare
```

### Python
```python
# Interactive debugging
import pdb; pdb.set_trace()  # Python 3.6-
breakpoint()                  # Python 3.7+

# In debugger:
# n (next line)
# s (step into)
# c (continue)
# p variable (print)
# l (list source)
# q (quit)

# Better debugger: ipdb
import ipdb; ipdb.set_trace()

# Traceback inspection
import traceback
try:
    risky_operation()
except Exception:
    traceback.print_exc()
```

### Go
```bash
# Delve debugger
dlv debug main.go
(dlv) break main.main
(dlv) continue
(dlv) step
(dlv) print variable
(dlv) goroutines
(dlv) stack

# Race detector
go run -race main.go
go test -race ./...
```

## 4. Logging Strategy

### Log Levels
```
ERROR   — something broke, needs immediate attention
WARN    — something unexpected, but not broken
INFO    — normal operations worth recording
DEBUG   — detailed info for troubleshooting
TRACE   — extremely detailed, function-level
```

### Structured Logging
```json
{
  "timestamp": "2024-01-15T10:30:00Z",
  "level": "error",
  "message": "Failed to process payment",
  "error": "Connection refused",
  "user_id": "usr_123",
  "order_id": "ord_456",
  "amount": 99.99,
  "provider": "stripe"
}
```

### What to Log
- **Errors**: Full stack trace, context variables, request ID
- **Requests**: Method, path, status code, duration
- **Business events**: User signup, payment, export
- **Performance**: Query duration, cache hit/miss, external API calls

### What NOT to Log
- ❌ Passwords, API keys, tokens
- ❌ Full request/response bodies (may contain PII)
- ❌ Sensitive user data (SSN, credit card numbers)
- ❌ Logging in hot loops (kills performance)

## 5. Debugging Tools

### Network Debugging
```bash
# Check DNS resolution
dig example.com
nslookup example.com

# Check connectivity
curl -v https://api.example.com/health
telnet api.example.com 443

# Capture traffic
tcpdump -i eth0 port 443
wireshark  # GUI packet analyzer

# Test API endpoints
curl -X POST https://api.example.com/users \
  -H "Content-Type: application/json" \
  -d '{"name": "test"}' \
  -v
```

### Process Debugging
```bash
# Find process using a port
lsof -i :3000
ss -tlnp | grep 3000

# strace a process
strace -p <pid> -e trace=network

# Check open file descriptors
ls -la /proc/<pid>/fd

# Check environment variables
cat /proc/<pid>/environ | tr '\0' '\n'
```

### Database Debugging
```sql
-- Check for locks
SELECT * FROM pg_locks WHERE NOT granted;

-- Check slow queries
SELECT * FROM pg_stat_activity WHERE state = 'active' AND query_start < NOW() - INTERVAL '5 minutes';

-- Check table sizes
SELECT relname, pg_size_pretty(pg_total_relation_size(relid))
FROM pg_catalog.pg_statio_user_tables
ORDER BY pg_total_relation_size(relid) DESC;

-- Check index usage
SELECT relname, indexrelname, idx_scan
FROM pg_stat_user_indexes
ORDER BY idx_scan ASC;
```

## 6. Preventing Future Bugs

### Add Regression Tests
```python
# Write a test that reproduces the bug
def test_user_email_uniqueness():
    """Regression: concurrent signups could create duplicate emails."""
    create_user(email="test@example.com")
    with pytest.raises(IntegrityError):
        create_user(email="test@example.com")
```

### Add Defensive Checks
```python
# Validate inputs at boundaries
def process_order(order_data: dict) -> Order:
    if not order_data.get("items"):
        raise ValueError("Order must have at least one item")
    if order_data["total"] < 0:
        raise ValueError("Order total cannot be negative")
    # ... proceed with processing
```

### Add Monitoring
```python
# Alert on conditions that caused the bug
if error_rate > 0.01:
    alert("Error rate exceeded 1% threshold")
if response_time_p99 > 2000:
    alert("P99 response time exceeded 2 seconds")
```

## 7. Debugging Checklist

When you find a bug:
- [ ] Can you reproduce it consistently?
- [ ] Have you identified the root cause (not just the symptom)?
- [ ] Is the fix minimal and targeted?
- [ ] Does the fix handle edge cases?
- [ ] Have you verified no regressions?
- [ ] Have you added a test to prevent recurrence?
- [ ] Have you checked for the same pattern elsewhere in the codebase?
- [ ] Have you documented the fix (commit message, PR description)?
