# kube-depod

Kubernetes operator for automated Pod cleanup based on annotation-driven policies.

## Overview

kube-depod is a Rust-based Kubernetes operator that automatically deletes Pods based on configurable `DepodPolicy` CRDs. It supports:

- **Annotation-driven triggers**: Policies activate when Pods have specific annotations
- **Flexible conditions**: TTL-based (Builtin) or CEL expression-based conditions
- **Safety guardrails**: Rate limiting, system namespace protection, dry-run mode
- **Observability**: Structured logging with tracing

## Architecture

```
DepodPolicy CRD
      ↓
kube-depod Operator
  - Watch Pods
  - Load & cache DepodPolicies
  - Match Pods against DepodPolicies
  - Evaluate Conditions
  - Execute Actions (Delete/Evict)
      ↓
Kubernetes API Server
```

## Phase 1-2: MVP + CEL Features (Current)

✅ **Completed:**
- DepodPolicy CRD definition with validation
- Pod watching and reconciliation loop
- Annotation-based policy triggers
- Builtin TTL condition evaluation
- CEL expression engine integration
  - Age-based conditions (`age > seconds`)
  - Phase conditions (`status.phase == "Failed"`)
  - Namespace conditions (`metadata.namespace == "ns"`)
- Delete action with graceful termination
- Dry-run mode
- System namespace protection
- Structured logging
- Unit tests (20+ test cases)

## Building

```bash
cargo build --release
```

## Installation

### Using Helm (Recommended)

```bash
# Add the Helm repository
helm repo add kube-depod https://mrchypark.github.io/kube-depod
helm repo update

# Install in kube-system namespace
helm install kube-depod kube-depod/kube-depod -n kube-system --create-namespace
```

For more Helm options, see [Helm Chart Documentation](helm/README.md)

## Running

### In-cluster

```bash
# Using kubectl
kubectl apply -f manifests/crd.yaml
kubectl apply -f manifests/rbac.yaml
kubectl apply -f manifests/deployment.yaml

# Or using Helm
helm install kube-depod ./helm/kube-depod -n kube-system --create-namespace
```

### Local Development

```bash
# Requires kind/minikube cluster
cargo check
cargo test
```

### Metrics Endpoints

The operator exposes Prometheus metrics on port 8080:

```bash
# Get Prometheus format metrics
curl http://localhost:8080/metrics

# Health check
curl http://localhost:8080/health
```

Example metrics output:
```
# HELP kube_depod_pods_evaluated_total Total number of pods evaluated
# TYPE kube_depod_pods_evaluated_total counter
kube_depod_pods_evaluated_total {} 42

# HELP kube_depod_pods_deleted_total Total number of pods deleted
# TYPE kube_depod_pods_deleted_total counter
kube_depod_pods_deleted_total {} 5

# HELP kube_depod_policy_matches_total Total number of policy matches
# TYPE kube_depod_policy_matches_total counter
kube_depod_policy_matches_total {} 8

# HELP kube_depod_evaluation_errors_total Total number of evaluation errors
# TYPE kube_depod_evaluation_errors_total counter
kube_depod_evaluation_errors_total {} 0

# HELP kube_depod_rate_limited_total Total number of rate limit hits
# TYPE kube_depod_rate_limited_total counter
kube_depod_rate_limited_total {} 2
```

### Rate Limiting

The operator includes token bucket rate limiting to prevent overwhelming the Kubernetes API:

- Default: 20 deletes per minute
- Configurable via environment or code
- Gracefully handles rate limit exceeding by skipping deletion but continuing to process other pods

## Example Policies

### Builtin TTL Policy
See `examples/ttl-policy.yaml`:
- Deletes Pods older than 10 minutes
- Uses builtin TTL condition
- Protects system namespaces

### CEL Expression Policies
See `examples/cel-policy.yaml`:
- **Failed pod cleanup**: Deletes Pods with `status.phase == "Failed"`
- **Old ephemeral pods**: Deletes Pods older than 30 minutes with label `ephemeral: true`
- Both policies support dry-run mode for testing

For CEL expression documentation, see `docs/CEL_EXPRESSIONS.md`

## Roadmap

### Phase 2: CEL Integration ✅ (Complete)
- ✅ CEL expression engine integration
- ✅ Pod context mapping (age, phase, namespace)
- ✅ Expression evaluation and caching

#### Phase 3: Observability ✅ (Complete)
- ✅ Prometheus metrics endpoint (`:8080/metrics`)
- ✅ Rate limiting implementation (token bucket, configurable per minute)
- ✅ Health check endpoint (`:8080/health`)
- ✅ Metrics tracking:
  - Total pods evaluated
  - Total pods deleted
  - Total policy matches
  - Total evaluation errors
  - Total rate limit hits

### Phase 4: Advanced Features
- Evict action support
- Multi-policy coordination
- Status field extensions

## Project Structure

```
src/
├── main.rs              # Entrypoint, Pod watcher, metrics collection
├── lib.rs               # Library root
├── crd.rs               # DepodPolicy CRD definition
├── controller.rs        # Reconciliation logic
├── error.rs             # Error types
├── metrics.rs           # Prometheus metrics collection
├── server.rs            # HTTP server for metrics/health endpoints
├── rate_limiter.rs      # Token bucket rate limiter
└── engine/
    ├── mod.rs           # Engine module
    └── cel.rs           # CEL expression evaluator
examples/
├── ttl-policy.yaml      # Example DepodPolicy and Pod
```

## Development

### Testing

```bash
cargo test
```

### Code Quality

```bash
cargo clippy
cargo fmt
```

## License

TBD
