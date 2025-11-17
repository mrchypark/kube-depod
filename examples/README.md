# kube-depod Policy Examples

This directory contains example DepodPolicy resources and test Pods for kube-depod.

## Files

### 1. ttl-policy.yaml
**Purpose**: Demonstrates TTL-based (Builtin) pod cleanup

- Uses the simple Builtin TTL condition type
- Deletes pods older than 10 minutes
- Best for basic time-based cleanup without complex logic
- Single policy with example pod

**Key Concept**:
```yaml
when:
  type: "Builtin"
  ttlSeconds: 600  # 10 minutes
```

### 2. cel-policy.yaml
**Purpose**: Demonstrates various CEL expression-based cleanup policies

Contains 5 example policies:
1. **CrashLoopBackOff detection**: Deletes pods with restart count >= 5
2. **Image pull errors**: Deletes pods with ImagePullBackOff or ErrImagePull
3. **High restart count**: Deletes pods with > 10 restarts and not ready
4. **Succeeded phase**: Deletes completed batch jobs
5. **Dry-run example**: Tests policy without actually deleting

**Key Concepts**:
- Uses `status.phase` for phase checking
- Uses `status.containerStatuses` for container state checking
- Uses shortcut variables (status, metadata)
- Demonstrates dry-run mode for testing

**Example Expression**:
```cel
status.containerStatuses.exists(c,
  has(c.state.waiting) &&
  c.state.waiting.reason == 'CrashLoopBackOff' &&
  c.restartCount >= 5
)
```

### 3. periodic-resync-example.yaml
**Purpose**: Demonstrates periodic resync with age-based cleanup

Shows how to enable periodic pod re-evaluation for time-based conditions:
- Example 1: TTL using Builtin condition
- Example 2: CEL with age-based deletion for completed pods older than 1 hour

**Key Concept**: Using `age` variable for time-based comparisons
```cel
has(status) &&
status.phase == 'Succeeded' &&
age > 3600  # 1 hour
```

**Configuration**: Requires environment variables for periodic resync:
```bash
RESYNC_ENABLE=true
RESYNC_INTERVAL_SECONDS=3600
```

### 4. advanced-cel-examples.yaml
**Purpose**: Comprehensive examples of advanced CEL expressions (NEW)

Contains 6 advanced policy examples:
1. **Age-based cleanup**: Delete ephemeral pods older than 30 minutes
2. **Combined age + phase**: Delete failed/succeeded pods after 2 hours
3. **Namespace filtering**: Delete pods in specific namespace older than 1 hour
4. **Container errors with labels**: Delete labeled pods with error states
5. **Complex multi-condition**: Multiple AND/OR conditions with age and namespace checks
6. **Time-window cleanup**: Delete test pods older than 6 hours

**Key Techniques**:
- Age-based conditions: `age > 1800`
- Phase checking: `status.phase == 'Failed' || status.phase == 'Succeeded'`
- Label checking: `metadata.labels['app'] == 'worker'`
- Namespace checking: `metadata.namespace == 'default'`
- Container state checks: `status.containerStatuses.exists(c, ...)`
- Complex logic: AND/OR combinations with multiple conditions

**Example Complex Expression**:
```cel
age > 300 &&
(
  status.phase == 'Failed' ||
  status.containerStatuses.exists(c,
    has(c.state.waiting) &&
    c.state.waiting.reason == 'ImagePullBackOff'
  )
) &&
metadata.namespace != 'prod'
```

## Available CEL Variables

All CEL policies can use these variables:

| Variable | Type | Description |
|----------|------|-------------|
| `pod` | Object | Full Pod object (root variable) |
| `metadata` | Object | pod.metadata (namespace, name, labels, annotations, creationTimestamp, etc.) |
| `spec` | Object | pod.spec (containers, volumes, nodeSelector, etc.) |
| `status` | Object | pod.status (phase, containerStatuses, conditions, etc.) |
| `now` | Int | Current Unix timestamp (epoch seconds, UTC) |
| `age` | Int | Seconds since pod creation (now - metadata.creationTimestamp) |

## Common CEL Patterns

### Phase-based checks
```cel
status.phase == 'Failed'
status.phase == 'Succeeded'
(status.phase == 'Failed' || status.phase == 'Succeeded')
```

### Age-based checks
```cel
age > 1800                    # Older than 30 minutes
age < 300                     # Younger than 5 minutes
age > 3600 && age < 7200      # Between 1 and 2 hours
```

### Container status checks
```cel
# Check if any container is in CrashLoopBackOff
status.containerStatuses.exists(c,
  has(c.state.waiting) &&
  c.state.waiting.reason == 'CrashLoopBackOff'
)

# Check restart count
status.containerStatuses.exists(c, c.restartCount > 5)

# Check if any container is not ready
status.containerStatuses.exists(c, c.ready == false)

# Check if all containers are not ready
status.containerStatuses.all(c, c.ready == false)
```

### Metadata checks
```cel
# Label checking
metadata.labels['app'] == 'worker'
metadata.labels['env'] == 'test'

# Namespace checking
metadata.namespace == 'default'
metadata.namespace != 'kube-system'

# Annotation checking
has(metadata.annotations['cleanup-at'])
metadata.annotations['cleanup-policy'] == 'ttl'
```

### Combined conditions
```cel
# Age + Phase
age > 3600 && status.phase == 'Failed'

# Labels + Container errors
metadata.labels['app'] == 'api' &&
status.containerStatuses.exists(c,
  has(c.state.waiting) &&
  c.state.waiting.reason == 'ImagePullBackOff'
)

# Namespace exclusion + multiple conditions
metadata.namespace != 'prod' &&
(status.phase == 'Failed' || age > 7200)
```

## Deployment Instructions

### Basic Builtin TTL Policy
```bash
# Apply the simple TTL policy
kubectl apply -f ttl-policy.yaml

# Test with example pod
# Pod will be deleted after 10 minutes
```

### CEL Expression Policies
```bash
# Apply all CEL policies
kubectl apply -f cel-policy.yaml

# All example pods will be evaluated against policies
# Policies with dryRun: true won't actually delete
```

### Advanced CEL Examples
```bash
# Apply advanced policies
kubectl apply -f advanced-cel-examples.yaml

# Create test pods to evaluate policies
```

### Periodic Resync (for age-based evaluation)
```bash
# Set environment variables in deployment
kubectl set env deployment/kube-depod \
  RESYNC_ENABLE=true \
  RESYNC_INTERVAL_SECONDS=3600 \
  -n kube-system

# Apply periodic resync example
kubectl apply -f periodic-resync-example.yaml
```

## Testing Policies

### Dry-run mode
Test policies without deleting:
```yaml
then:
  type: "Delete"
  dryRun: true  # Enable dry-run
```

Check logs:
```bash
kubectl logs -f deployment/kube-depod -n kube-system | grep "Would delete"
```

### Debugging CEL Expressions
Enable debug logging:
```bash
kubectl set env deployment/kube-depod \
  RUST_LOG=debug \
  -n kube-system

kubectl logs -f deployment/kube-depod -n kube-system | grep "CEL evaluation"
```

## Migration from Old CEL Context

If you have existing policies using old variable names:

| Old Variable | New Variable | Updated Expression |
|--------------|--------------|-------------------|
| `object` | `pod` | `object.status` → `pod.status` |
| `self` | `pod` | `self.status` → `pod.status` |
| `namespace` | `metadata.namespace` | `namespace == 'default'` → `metadata.namespace == 'default'` |
| `name` | `metadata.name` | `name == 'pod-1'` → `metadata.name == 'pod-1'` |

**Before**:
```yaml
expression: |
  self.status.phase == 'Failed' &&
  namespace == 'default'
```

**After**:
```yaml
expression: |
  pod.status.phase == 'Failed' &&
  metadata.namespace == 'default'
```

## Related Documentation

- [Main README](../README.md) - Overall project documentation
- [CHANGES_SUMMARY.md](../CHANGES_SUMMARY.md) - Detailed CEL context redesign information
