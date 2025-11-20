# Release Notes (v0.3.2)

**Changes since v0.3.0**

## 🔧 CI/CD

- **Fixed Workflow Trigger**: Updated the tagging workflow to use a Personal Access Token (PAT). This ensures that the `build-and-push` workflow is correctly triggered when a new version tag is pushed.

## 🚀 Features & Improvements

- **Robust Policy Namespace Resolution**: Improved the logic for determining the namespace of a `DepodPolicy`.
    - The controller now attempts to infer the policy's namespace from its `namespaceSelector` if the namespace is missing in the metadata (which can happen during certain API operations).
    - This fixes the `ApiError: NotFound` issue that occurred when the controller tried to update the status of a policy but couldn't correctly identify its namespace.

- **Enhanced CEL Evaluation Robustness**: Improved the reliability of CEL policy evaluation.
    - The `status` variable is now always available in the CEL evaluation context, even for pods that do not yet have a status (e.g., newly created pods).
    - Instead of failing with an "undeclared reference" error, missing pod status is treated as an empty object. This allows expressions like `has(status.phase)` or `status.reason` to be evaluated safely without causing controller errors.

## 🐛 Bug Fixes

- Fixed an issue where the controller would log errors and fail to update policy status due to incorrect namespace resolution.
- Fixed CEL evaluation errors for pods with missing status fields.

## 🧪 Tests

- Added unit tests for `get_policy_namespace` to verify fallback logic.
- Added tests for CEL `status` variable initialization.
