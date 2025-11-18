# Release Notes - v0.3.0

## 🚀 New Features

### Per-Policy Rate Limiting
- **Feature**: Added support for the `maxDeletesPerMinute` field in `DepodPolicy`.
- **Description**: You can now configure rate limits specifically for individual policies. This works in conjunction with the global rate limit.
    - If a policy has `maxDeletesPerMinute` set, both the global limit AND the policy limit must be satisfied.
    - If a policy does not have it set, only the global limit applies.
- **Usage**:
  ```yaml
  spec:
    limits:
      maxDeletesPerMinute: 10
  ```

## 🛠 Improvements

### Controller Logic Refactoring
- **Refactor**: Extracted TTL calculation and requeue logic into a dedicated helper function `calculate_ttl_requeue`.
- **Refactor**: Improved `evaluate_ttl_condition` to accept a `now` parameter, enabling deterministic testing.
- **Refactor**: Updated `reconcile_pod` to use the new helper functions and improve readability.

### Documentation
- **Update**: Clarified rate limiting behavior in `README.md`.
- **Update**: Consolidated "Features" and simplified "Roadmap" in `README.md`.
- **Update**: Updated example YAML files to reflect the new rate limiting capabilities.

## 🐛 Bug Fixes
- **Fix**: Removed the validation warning for `maxDeletesPerMinute` as it is now fully implemented.
