# kube-depod

Kubernetes operator for automated Pod cleanup based on annotation-driven policies.

## Overview

kube-depod is a Rust-based Kubernetes operator that automatically deletes Pods based on configurable `DepodPolicy` CRDs. It supports:

- **Annotation-driven triggers**: Policies activate when Pods have specific annotations
- **Flexible conditions**: TTL-based (Builtin) or CEL expression-based conditions
- **Safety guardrails**: Rate limiting, system namespace protection (kube-system, kube-public, kube-node-lease, kube-depod), dry-run mode
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

## Features

✅ **Core Functionality:**
- DepodPolicy CRD definition with validation
- Pod watching and reconciliation loop
- Annotation-based policy triggers
- Builtin TTL condition evaluation
- Delete action with graceful termination
- Dry-run mode
- System namespace protection (prevents accidental deletion in kube-system, kube-public, kube-node-lease, kube-depod)

✅ **CEL Integration:**
- CEL expression engine integration
- Pod context mapping (age, phase, namespace)
- Expression evaluation and caching
- Supported conditions:
  - Age-based (`age > seconds`)
  - Phase-based (`status.phase == "Failed"`)
  - Namespace-based (`metadata.namespace == "ns"`)

✅ **Observability & Safety:**
- Prometheus metrics endpoint (`:8080/metrics`)
- Health check endpoint (`:8080/health`)
- Rate limiting (token bucket, configurable per minute)
- Structured logging with tracing
- Metrics tracking (evaluated, deleted, matched, errors, rate limited)

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

# Install in kube-depod namespace (isolated from system namespaces)
helm install kube-depod kube-depod/kube-depod -n kube-depod --create-namespace
```

For more Helm options, see [Helm Chart Documentation](helm/README.md)

## Running

### In-cluster

```bash
# Using kubectl
kubectl apply -f manifests/crd.yaml
kubectl apply -f manifests/rbac.yaml
kubectl apply -f manifests/deployment.yaml

# Or using Helm (recommended)
helm install kube-depod ./helm/kube-depod -n kube-depod --create-namespace
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

**Note**: The `maxDeletesPerMinute` field in the `DepodPolicy` CRD allows you to set a specific rate limit for that policy. This works in conjunction with the global rate limit:
- If a policy has `maxDeletesPerMinute` set, both the global limit AND the policy limit must be satisfied.
- If a policy does not have it set, only the global limit applies.

### Pod Patch Concurrency Limit

When a policy is updated, the operator triggers re-evaluation of all matching Pods by patching their annotations (a "touch" operation). To prevent overwhelming the API server with concurrent requests:

- Default: 10 concurrent patch operations
- Configurable via `POD_PATCH_CONCURRENCY_LIMIT` environment variable
- Limits parallel pod patch operations during policy reconciliation
- Helps distribute API load when policies affect thousands of pods

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

## CEL Variables and Expressions

The CEL evaluator provides a consistent set of variables for policy expressions:

| Variable | Type | Description |
|----------|------|-------------|
| `pod` | Object | Full Pod object (root variable) |
| `metadata` | Object | Shortcut for `pod.metadata` |
| `spec` | Object | Shortcut for `pod.spec` |
| `status` | Object | Shortcut for `pod.status` |
| `now` | Int | Current timestamp (epoch seconds, UTC) |
| `age` | Int | Seconds since pod creation (creationTimestamp) |

### Example Expressions

```cel
# Phase-based cleanup
status.phase == 'Succeeded'

# Age-based cleanup (pods older than 30 minutes)
age > 1800

# Container restart count check
status.containerStatuses.exists(c, c.restartCount > 10)

# Container error state check
status.containerStatuses.exists(c,
  has(c.state.waiting) &&
  c.state.waiting.reason == 'CrashLoopBackOff'
)

# Combined conditions
status.phase == 'Failed' && age > 3600

# Metadata access
metadata.namespace == 'default' && metadata.labels['app'] == 'worker'
```

### Time Handling

- `now`: Current Unix timestamp (seconds since epoch, UTC)
- `age`: Calculated as `now - pod.metadata.creationTimestamp`, protected against clock skew (minimum 0)
- For time comparisons, use `age > seconds` for relative age checks

## Roadmap

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

This project is licensed under the Apache License 2.0. See the [LICENSE](LICENSE) file for details.
