# CEL Expression Guide for kube-depod

This document describes the CEL (Common Expression Language) expressions supported by kube-depod for policy conditions.

## Overview

CEL expressions are evaluated against Pod objects to determine if a deletion policy should be triggered. The current implementation supports a simplified set of CEL patterns that cover most common use cases.

## Supported Patterns

### 1. Age-based Conditions

Check if a Pod is older than a specified number of seconds.

**Pattern:** `age > SECONDS`

**Examples:**
```cel
age > 600          # Pod older than 10 minutes
age > 1800         # Pod older than 30 minutes
age > 3600         # Pod older than 1 hour
age > 86400        # Pod older than 1 day
```

**Use Case:** Remove stale Pods that have been running for too long.

---

### 2. Phase Conditions

Check the Pod's current status phase.

**Pattern:** `status.phase == "PHASE"`

**Supported Phases:**
- `"Failed"` - Pod is in Failed state
- `"Pending"` - Pod is still pending (not yet running)
- `"Unknown"` - Pod state is unknown
- `"CrashLoopBackOff"` - Container crash loop detected

**Examples:**
```cel
status.phase == "Failed"          # Failed Pods
status.phase == "Pending"         # Pending Pods
status.phase == "Unknown"         # Unknown phase Pods
status.phase == "CrashLoopBackOff" # Crash loop Pods
```

**Use Case:** Clean up Pods in problematic states.

---

### 3. Namespace Conditions

Check if a Pod is in a specific namespace.

**Pattern:** `metadata.namespace == "NAMESPACE"`

**Examples:**
```cel
metadata.namespace == "default"    # Pods in default namespace
metadata.namespace == "testing"    # Pods in testing namespace
metadata.namespace == "staging"    # Pods in staging namespace
```

**Use Case:** Apply different cleanup policies to different namespaces.

---

## Combined Conditions (Future)

In a future release, logical operators (AND/OR) will be supported:

```cel
# Hypothetical examples
age > 600 && status.phase == "Failed"
metadata.namespace == "testing" || metadata.namespace == "staging"
```

---

## Implementation Details

### Pod Context

When evaluating expressions, the following Pod data is available:

```json
{
  "metadata": {
    "name": "pod-name",
    "namespace": "default",
    "creationTimestamp": "2024-11-14T12:00:00Z",
    "labels": {
      "app": "my-app"
    }
  },
  "status": {
    "phase": "Running"
  }
}
```

### Built-in Variables

- `age`: Pod age in seconds (auto-calculated from `creationTimestamp`)
- `now`: Current UTC timestamp
- `metadata.*`: Pod metadata fields
- `status.*`: Pod status fields

---

## Examples

### Example 1: Cleanup Failed Pods

```yaml
apiVersion: kube-depod.io/v1alpha1
kind: PolicyRule
metadata:
  name: cleanup-failed
spec:
  target:
    namespaceSelector:
      matchNames: ["default"]
  trigger:
    annotationKey: "kube-depod/policy"
    annotationValues: ["cleanup-failed"]
  condition:
    type: "CEL"
    expression: "status.phase == \"Failed\""
  action:
    type: "Delete"
    gracePeriodSeconds: 30
    dryRun: false
```

### Example 2: Cleanup Old Test Pods

```yaml
apiVersion: kube-depod.io/v1alpha1
kind: PolicyRule
metadata:
  name: cleanup-old-test
spec:
  target:
    namespaceSelector:
      matchNames: ["testing"]
    podSelector:
      matchLabels:
        environment: "test"
  trigger:
    annotationKey: "kube-depod/cleanup"
    annotationValues: ["auto"]
  condition:
    type: "CEL"
    expression: "age > 3600"  # 1 hour
  action:
    type: "Delete"
    gracePeriodSeconds: 30
    dryRun: false
```

### Example 3: Dry-run Mode Testing

```yaml
apiVersion: kube-depod.io/v1alpha1
kind: PolicyRule
metadata:
  name: test-policy-dryrun
spec:
  target:
    namespaceSelector:
      matchNames: ["default"]
  trigger:
    annotationKey: "kube-depod/test"
    annotationValues: ["true"]
  condition:
    type: "CEL"
    expression: "age > 600"
  action:
    type: "Delete"
    gracePeriodSeconds: 0
    dryRun: true  # Will log what would be deleted, but not actually delete
```

---

## Troubleshooting

### Expression Not Evaluating as Expected

1. Check the operator logs for parsing errors
2. Ensure the expression follows one of the supported patterns
3. Use dry-run mode to test before enabling actual deletion

### Common Mistakes

- Missing quotes: `status.phase == Failed` (wrong) vs `status.phase == "Failed"` (correct)
- Invalid operators: `age > 600 && status.phase == "Failed"` (AND not yet supported)
- Typos: `metadata.namespace` (correct) vs `metadata.ns` (wrong)

---

## Roadmap

Future versions will support:

- Full CEL expression parser (via cel-interpreter or similar)
- Logical operators (AND, OR, NOT)
- Container-level conditions (e.g., restart count)
- Custom variables and functions
- Policy priorities and conditional logic
