# kube-depod

Kubernetes operator for automated Pod cleanup based on annotation-driven policies.

## Overview

kube-depod is a Rust-based Kubernetes operator that automatically deletes Pods based on configurable `PolicyRule` CRDs. It supports:

- **Annotation-driven triggers**: Policies activate when Pods have specific annotations
- **Flexible conditions**: TTL-based (Builtin) or CEL expression-based conditions
- **Safety guardrails**: Rate limiting, system namespace protection, dry-run mode
- **Observability**: Structured logging with tracing

## Architecture

```
PolicyRule CRD
      ↓
kube-depod Operator
  - Watch Pods
  - Load & cache PolicyRules
  - Match Pods against Policies
  - Evaluate Conditions
  - Execute Actions (Delete/Evict)
      ↓
Kubernetes API Server
```

## Phase 1-2: MVP + CEL Features (Current)

✅ **Completed:**
- PolicyRule CRD definition with validation
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

## Running

### In-cluster

```bash
kubectl apply -f examples/ttl-policy.yaml
cargo run --bin operator
```

### Local Development

```bash
# Requires kind/minikube cluster
cargo check
cargo test
```

## Example PolicyRules

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

### Phase 3: Observability
- Prometheus metrics
- Rate limiting implementation
- Enhanced logging

### Phase 4: Advanced Features
- Evict action support
- Multi-policy coordination
- Status field extensions

## Project Structure

```
src/
├── main.rs          # Entrypoint, Pod watcher
├── lib.rs           # Library root
├── crd.rs           # PolicyRule CRD definition
├── controller.rs    # Reconciliation logic
├── error.rs         # Error types
examples/
├── ttl-policy.yaml  # Example PolicyRule and Pod
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
