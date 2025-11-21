# AI Agent Usage Guidelines

This document provides guidelines for AI agents (like Claude, GPT, etc.) working on the `kube-depod` project.

## Version Management

### When Bumping Version

When updating the project version, you must update **two files manually**:

1. **`Cargo.toml`** - Update the `version` field
2. **`RELEASE_NOTES.md`** - Add a new version section with changes

**DO NOT** manually update `helm/kube-depod/Chart.yaml` - the workflow handles this.

**Example workflow**:

```toml
# 1. Update Cargo.toml
[package]
name = "kube-depod"
version = "0.3.5"  # Bump version here
```

```markdown
# 2. Update RELEASE_NOTES.md
## v0.3.5

**Changes**

- **Added Regression Test for Infinite Loop Fix**:
    - Created `tests/infinite_loop_regression_test.rs`
    - Test ensures status updates are only sent when necessary
```

**What the workflow does** (`.github/workflows/tag-on-version-change.yml`):
- Detects the version change in `Cargo.toml`
- Updates `Chart.yaml` (version and appVersion) automatically
- Updates `manifests/deployment.yaml` (image tag and version labels) automatically
- Commits and pushes the Chart.yaml and deployment.yaml changes
- Creates a git tag (e.g., `v0.3.5`)

## Semantic Versioning

Follow [Semantic Versioning 2.0.0](https://semver.org/):

- **MAJOR** (x.0.0): Breaking changes
- **MINOR** (0.x.0): New features, backward compatible
- **PATCH** (0.0.x): Bug fixes, backward compatible

### Examples

- `0.3.4` → `0.3.5`: Bug fix or minor improvement (regression test added)
- `0.3.5` → `0.4.0`: New feature (e.g., new policy type)
- `0.4.0` → `1.0.0`: Breaking change (e.g., CRD schema change)

## Testing

### Running Tests

```bash
# All tests
cargo test

# Specific test
cargo test --test infinite_loop_regression_test

# Integration tests (requires cluster)
cargo test --test controller_integration_test -- --ignored
```

### Adding Tests

- **Unit tests**: Add to `src/` files using `#[cfg(test)]` modules
- **Integration tests**: Add to `tests/` directory
- **Mock-based tests**: Use `tower-test` for mocking Kubernetes API (see `tests/infinite_loop_regression_test.rs`)

## Code Quality

### Before Committing

Run all checks from the test workflow to ensure CI will pass:

```bash
# 1. Run clippy (linting)
cargo clippy --all-targets --all-features -- -D warnings

# 2. Run unit tests
cargo test --all-features

# 3. Run integration tests (requires K3s/kind cluster)
cargo test --test controller_integration_test -- --ignored

# 4. Check compilation
cargo check

# 5. Format code
cargo fmt
```

**Required checks** (must pass before committing):
- ✅ `cargo clippy` - No warnings
- ✅ `cargo test` - All unit tests pass
- ✅ `cargo check` - Compilation successful
- ✅ Include `Cargo.lock` when bumping version and committing.

**Optional checks** (recommended):
- `cargo test --test controller_integration_test -- --ignored` - Integration tests (requires cluster)
- `cargo fmt` - Code formatting

> **Note**: The CI workflow (`.github/workflows/test.yml`) runs all these checks automatically on push and PR.

## Commit Message Format

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types**:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `test`: Adding or updating tests
- `refactor`: Code refactoring
- `chore`: Maintenance tasks

**Examples**:
```
feat: add regression test for infinite loop fix and bump version to v0.3.5
fix: resolve infinite reconciliation loop and bump version to v0.3.4
docs: update README with new installation instructions
```

## Workflow Understanding

### Tag-on-Version-Change Workflow

Located at `.github/workflows/tag-on-version-change.yml`:

1. Triggers on push to `main` branch
2. Detects version changes in `Cargo.toml`
3. Updates `Chart.yaml` and `RELEASE_NOTES.md`
4. Commits and pushes changes
5. Creates and pushes a git tag

### Build-and-Push Workflow

Located at `.github/workflows/build-and-push.yml`:

1. Triggers on tag push (e.g., `v0.3.5`)
2. Builds Docker image
3. Pushes to container registry
4. Creates GitHub release

## Common Tasks

### Adding a New Feature

1. Create a feature branch
2. Implement the feature
3. Add tests
4. Update `Cargo.toml` version (MINOR bump)
5. Update `RELEASE_NOTES.md` with new version section
6. Commit with `feat:` prefix
7. Push and create PR
8. After merge, workflow creates tag and updates Chart.yaml

### Fixing a Bug

1. Create a bugfix branch
2. Fix the bug
3. Add regression test
4. Update `Cargo.toml` version (PATCH bump)
5. Update `RELEASE_NOTES.md` with new version section
6. Commit with `fix:` prefix
7. Push and create PR
8. After merge, workflow creates tag and updates Chart.yaml

### Adding Documentation

1. Update relevant `.md` files
2. Commit with `docs:` prefix
3. No version bump needed for docs-only changes

## Project Structure

```
kube-depod/
├── src/
│   ├── main.rs           # Entry point
│   ├── controller.rs     # Reconciliation logic
│   ├── crd.rs            # CRD definitions
│   ├── engine/           # CEL evaluation
│   └── ...
├── tests/
│   ├── controller_integration_test.rs
│   └── infinite_loop_regression_test.rs
├── helm/
│   └── kube-depod/
│       ├── Chart.yaml    # Auto-updated by workflow
│       └── templates/
├── manifests/
│   └── deployment.yaml   # Auto-updated by workflow
├── .github/
│   └── workflows/
│       ├── tag-on-version-change.yml
│       └── build-and-push.yml
├── Cargo.toml            # Update version here
├── RELEASE_NOTES.md      # Update manually when bumping version
└── AGENTS.md             # This file
```

## Important Notes

- **When bumping version**: Update both `Cargo.toml` AND `RELEASE_NOTES.md` manually
- **Never manually edit** `Chart.yaml` - the workflow handles this automatically
- **Always run** `cargo check` and `cargo test` before committing
- **Use semantic versioning** strictly
- **Write meaningful commit messages** following Conventional Commits
- **Add tests** for new features and bug fixes
