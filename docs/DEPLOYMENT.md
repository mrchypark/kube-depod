# Deployment Guide

This guide explains how to deploy kube-depod to a Kubernetes cluster.

## Prerequisites

- Kubernetes 1.20+
- kubectl configured to access your cluster
- (Optional) Docker for building images

## Quick Start with Kustomize

```bash
# Install CRD and deploy operator
kubectl apply -k manifests/

# Verify deployment
kubectl -n kube-system get deployment kube-depod
kubectl -n kube-system get pod -l app=kube-depod
kubectl -n kube-system logs -l app=kube-depod --tail=50
```

## Manual Installation

### 1. Create CRD

```bash
kubectl apply -f manifests/crd.yaml
```

### 2. Create RBAC Resources

```bash
kubectl apply -f manifests/rbac.yaml
```

### 3. Build and Push Image

```bash
# Build the Docker image
docker build -t myregistry/kube-depod:v0.1.0 .

# Push to registry
docker push myregistry/kube-depod:v0.1.0
```

### 4. Update Image in Deployment

Edit `manifests/deployment.yaml`:

```yaml
image: myregistry/kube-depod:v0.1.0
imagePullPolicy: IfNotPresent
```

### 5. Deploy

```bash
kubectl apply -f manifests/deployment.yaml
```

## Local Development with Kind

### 1. Create a Kind Cluster

```bash
kind create cluster --name kube-depod-dev
```

### 2. Build and Load Image

```bash
# Build release binary
cargo build --release

# Build Docker image
docker build -t kube-depod:latest .

# Load into Kind cluster
kind load docker-image kube-depod:latest --name kube-depod-dev
```

### 3. Install and Verify

```bash
# Switch to Kind context
kubectl cluster-info --context kind-kube-depod-dev

# Install with Kustomize
kubectl apply -k manifests/

# Check logs
kubectl -n kube-system logs -f -l app=kube-depod
```

### 4. Test with Example Policies

```bash
# Create test namespace
kubectl create namespace test

# Apply test policies
kubectl apply -f examples/ttl-policy.yaml
kubectl apply -f examples/cel-policy.yaml

# Check created policies
kubectl get policyrules -A

# Monitor logs for matches
kubectl -n kube-system logs -f -l app=kube-depod
```

## Uninstall

```bash
# Using Kustomize
kubectl delete -k manifests/

# Or manual cleanup
kubectl delete deployment -n kube-system kube-depod
kubectl delete serviceaccount -n kube-system kube-depod
kubectl delete clusterrolebinding kube-depod
kubectl delete clusterrole kube-depod
kubectl delete crd policyrules.kube-depod.io
```

## Configuration

### Environment Variables

- `RUST_LOG`: Logging level (default: `info`)
  - Options: `trace`, `debug`, `info`, `warn`, `error`
  - Example: `RUST_LOG=kube_depod=debug,kube=info`

### Security Considerations

1. **ServiceAccount**: Operator runs as non-root user (UID 65534)
2. **RBAC**: Minimal permissions granted
   - Can read PolicyRules and Pods
   - Can delete Pods
   - Cannot modify other resources
3. **Pod Security**: Read-only root filesystem where possible
4. **Network**: No external network access required (in-cluster only)

## Monitoring

### Check Operator Status

```bash
# Verify deployment is running
kubectl -n kube-system get deployment kube-depod

# Check pod status
kubectl -n kube-system get pod -l app=kube-depod

# View operator logs
kubectl -n kube-system logs -l app=kube-depod
```

### Test Policies

Create a test Pod to verify policies work:

```bash
# Create with TTL policy annotation
kubectl run test-pod --image=busybox:latest \
  -n default \
  -l app=my-app \
  --annotations="kube-depod/policy=ttl-10m" \
  -- sleep 3600

# Check if policy matches
kubectl -n kube-system logs -l app=kube-depod | grep "test-pod"
```

### Dry-run Mode

Always test new policies with `dryRun: true`:

```yaml
action:
  type: Delete
  dryRun: true  # Enable this first
```

Check logs to see what would be deleted:

```bash
kubectl -n kube-system logs -l app=kube-depod | grep "DRY RUN"
```

Once verified, set `dryRun: false` to enable actual deletions.

## Troubleshooting

### Operator not starting

```bash
# Check deployment
kubectl -n kube-system describe deployment kube-depod

# Check pod events
kubectl -n kube-system describe pod -l app=kube-depod

# Check logs
kubectl -n kube-system logs -l app=kube-depod
```

### Policies not matching

```bash
# Verify PolicyRule exists
kubectl get policyrules -A

# Check policy details
kubectl describe policyrule <name> -n <namespace>

# Enable debug logging
# Edit deployment and set RUST_LOG=debug
kubectl -n kube-system set env deployment/kube-depod RUST_LOG=kube_depod=debug
kubectl -n kube-system rollout restart deployment/kube-depod
```

### Pods not being deleted

1. Verify policy is in dry-run mode first
2. Check that Pod annotation matches policy trigger
3. Check that condition evaluates to true
4. Verify system namespace protection isn't blocking deletion
5. Check RBAC permissions

### High memory usage

The operator caches PolicyRules in memory. With many policies (>1000), consider:

- Reducing policy scope with better namespaceSelector/podSelector
- Increasing memory limits in deployment
- Splitting policies across multiple operator instances

## Upgrade

```bash
# Build new version
docker build -t myregistry/kube-depod:v0.2.0 .
docker push myregistry/kube-depod:v0.2.0

# Update deployment image
kubectl -n kube-system set image deployment/kube-depod \
  operator=myregistry/kube-depod:v0.2.0

# Watch rollout
kubectl -n kube-system rollout status deployment/kube-depod
```

## HA Setup (Future)

For production environments, consider deploying multiple replicas:

```yaml
replicas: 3
affinity:
  podAntiAffinity:
    requiredDuringSchedulingIgnoredDuringExecution:
    - labelSelector:
        matchExpressions:
        - key: app
          operator: In
          values:
          - kube-depod
      topologyKey: kubernetes.io/hostname
```

Note: This is currently not fully tested; use with caution.
