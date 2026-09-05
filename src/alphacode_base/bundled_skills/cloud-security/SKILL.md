---
name: cloud-security
description: Cloud security assessment: IAM misconfigurations, S3 bucket exposure, SSRF via cloud metadata, container escape, and Kubernetes RBAC bypass.
---

# Cloud Security Skill

Cloud infrastructure security assessment.

## AWS/GCP/Azure Checklist

- **IAM**: least privilege, no wildcard permissions, MFA on root
- **Storage**: S3/GCS buckets not public, encryption at rest
- **Network**: VPC properly configured, security groups tight
- **Logging**: CloudTrail/audit logs enabled and monitored
- **Secrets**: not in environment variables, use KMS/Secrets Manager

## SSRF to Cloud Metadata

- `http://169.254.169.254/latest/meta-data/` (AWS)
- `http://metadata.google.internal/` (GCP)
- `http://169.254.169.254/metadata/instance` (Azure, with header)

## Container Security

- Run as non-root user
- Read-only root filesystem
- Drop all capabilities, add only needed ones
- No privileged containers
- Scan images for vulnerabilities

## Kubernetes

- RBAC: no cluster-admin unless necessary
- Network policies to restrict pod-to-pod traffic
- Pod security standards (restricted profile)
- Secrets encrypted at rest in etcd
