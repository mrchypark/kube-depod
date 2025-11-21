# Release Notes

## v0.3.6

**Changes**

- **Automated Version Management**:
    - Updated workflow to automatically update `manifests/deployment.yaml` when version changes
    - Workflow now updates image tags and version labels in deployment manifests
- **Documentation**:
    - Created `AGENTS.md` with comprehensive AI agent usage guidelines
    - Clarified version management process (manual: Cargo.toml + RELEASE_NOTES.md, automatic: Chart.yaml + deployment.yaml)

## v0.3.5

**Changes**

- **Added Regression Test for Infinite Loop Fix**:
    - Created `tests/infinite_loop_regression_test.rs` using mock client to verify that the infinite loop fix from v0.3.4 works correctly.
    - Test ensures that status updates are only sent when necessary, preventing redundant API calls.
    - Added test dependencies: `tower-test`, `tower`, `http`, `http-body-util`.

## v0.3.4

**Changes**

- **Fixed Infinite Reconciliation Loop**:
    - Implemented "check-before-update" logic in `reconcile_policy` to prevent unconditional status updates that caused self-triggering loops.
    - Added early CEL validation in `reconcile_policy` to catch compilation errors before they enter the reconciliation cycle.
    - Removed policy status updates from `reconcile_pod` to prevent "ping-pong" loops between the Pod and Policy controllers during error conditions.

## v0.3.3

**Changes**

- **CI/CD Fix**: Removed `[skip ci]` from the release commit message and ensured explicit PAT usage. This fixes the issue where the tag push event was being suppressed, preventing the build workflow from triggering.

## v0.3.2

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
