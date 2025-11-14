# kube-depod Helm Chart

Kubernetes operator for automated Pod cleanup based on annotation-driven policies.

## Installation

### Prerequisites

- Kubernetes 1.20+
- Helm 3.0+

### Add Helm Repository

```bash
# If hosting on GitHub Pages or similar
helm repo add kube-depod https://mrchypark.github.io/kube-depod
helm repo update
```

### Install the Chart

```bash
# Install with default values
helm install kube-depod ./helm/kube-depod

# Install in a specific namespace
helm install kube-depod ./helm/kube-depod -n kube-system --create-namespace

# Install with custom values
helm install kube-depod ./helm/kube-depod -f values.yaml
```

### Upgrade the Chart

```bash
helm upgrade kube-depod ./helm/kube-depod
```

### Uninstall the Chart

```bash
helm uninstall kube-depod

# Remove CRDs (optional, be careful as it will delete all Policy resources)
kubectl delete crd policies.kube-depod.io
```

## Configuration

### Common Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `replicaCount` | Number of operator replicas | `1` |
| `image.repository` | Docker image repository | `ghcr.io/mrchypark/kube-depod` |
| `image.tag` | Docker image tag | Chart AppVersion |
| `image.pullPolicy` | Image pull policy | `IfNotPresent` |
| `namespace.create` | Create namespace | `true` |
| `namespace.name` | Namespace name | `kube-system` |

### Service Account

| Parameter | Description | Default |
|-----------|-------------|---------|
| `serviceAccount.create` | Create service account | `true` |
| `serviceAccount.annotations` | Service account annotations | `{}` |
| `serviceAccount.name` | Service account name | Auto-generated |

### RBAC

| Parameter | Description | Default |
|-----------|-------------|---------|
| `rbac.create` | Create RBAC resources | `true` |

### Operator Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `operator.rateLimit` | Max deletes per minute | `20` |
| `operator.protectSystemNamespaces` | Protect system namespaces | `true` |
| `operator.metricsPort` | Metrics server port | `8080` |

### Pod Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `resources.limits.cpu` | CPU limit | `500m` |
| `resources.limits.memory` | Memory limit | `256Mi` |
| `resources.requests.cpu` | CPU request | `100m` |
| `resources.requests.memory` | Memory request | `128Mi` |
| `podSecurityContext` | Pod security context | Non-root user |
| `securityContext` | Container security context | Restrictive |

### Autoscaling

| Parameter | Description | Default |
|-----------|-------------|---------|
| `autoscaling.enabled` | Enable autoscaling | `false` |
| `autoscaling.minReplicas` | Minimum replicas | `1` |
| `autoscaling.maxReplicas` | Maximum replicas | `3` |
| `autoscaling.targetCPUUtilizationPercentage` | Target CPU usage | `80` |

### CRDs

CRDs are automatically installed from the `crds/` directory in the chart.

| Parameter | Description | Default |
|-----------|-------------|---------|
| `crds.enabled` | Install Policy CRD | `true` |

## Examples

### Install with Custom Resource Limits

```bash
helm install kube-depod kube-depod/kube-depod \
  --set resources.limits.cpu=1000m \
  --set resources.limits.memory=512Mi
```

### Install with Autoscaling

```bash
helm install kube-depod kube-depod/kube-depod \
  --set autoscaling.enabled=true \
  --set autoscaling.minReplicas=2 \
  --set autoscaling.maxReplicas=5
```

### Install with Custom Image

```bash
helm install kube-depod kube-depod/kube-depod \
  --set image.repository=myregistry.azurecr.io/kube-depod \
  --set image.tag=latest
```

## Verifying Installation

```bash
# Check deployment
kubectl get deployment -n kube-system kube-depod

# Check Pod
kubectl get pod -n kube-system -l app.kubernetes.io/name=kube-depod

# Check CRD
kubectl get crd policies.kube-depod.io

# Check metrics
kubectl port-forward -n kube-system svc/kube-depod-metrics 8080:8080
curl http://localhost:8080/metrics

# Check health
curl http://localhost:8080/health
```

## Creating Policies

After installation, you can create Policy resources:

```yaml
apiVersion: kube-depod.io/v1alpha1
kind: Policy
metadata:
  name: ttl-10m-policy
  namespace: default
spec:
  target:
    namespaceSelector:
      matchNames:
        - default
    podSelector:
      matchLabels:
        app: my-app
  trigger:
    annotationKey: "kube-depod/policy"
    annotationValues:
      - "ttl-10m"
  condition:
    type: "Builtin"
    ttlSeconds: 600
  action:
    type: "Delete"
    gracePeriodSeconds: 30
    dryRun: false
  limits:
    maxDeletesPerMinute: 20
    protectSystemNamespaces: true
```

## Troubleshooting

### Check Logs

```bash
kubectl logs -n kube-system -l app.kubernetes.io/name=kube-depod -f
```

### Debug Mode

```bash
helm install kube-depod kube-depod/kube-depod \
  --set env.RUST_LOG=debug
```

### Dry-Run Mode

Test policies without actual deletion:

```yaml
action:
  type: "Delete"
  dryRun: true
```
