---
name: crypto-audit
description: Cryptographic implementation audit: weak algorithms, key management flaws, TLS misconfigurations, JWT vulnerabilities, and random number generation issues.
---

# Crypto Audit Skill

Audit cryptographic implementations for weaknesses.

## Checklist

- **Algorithms**: no MD5, SHA1 for security; use SHA256+, AES-256, RSA-2048+
- **Key Management**: keys not hardcoded, proper rotation, secure storage (HSM/KMS)
- **Randomness**: CSPRNG for security purposes, not Math.random() or time-based
- **TLS**: TLS 1.2+ only, strong cipher suites, certificate validation
- **JWT**: asymmetric signing, short expiration, audience/issuer validation
- **Hashing**: bcrypt/scrypt/argon2 for passwords, not plain SHA256

## Common Vulnerabilities

- Hardcoded encryption keys in source code
- ECB mode for block ciphers (use CBC/GCM)
- Padding oracle attacks on CBC mode
- Weak random number generators for tokens/secrets
- Missing certificate pinning
- Deprecated TLS versions (1.0, 1.1)
- Insufficient key length (RSA < 2048)
- IV reuse in symmetric encryption
