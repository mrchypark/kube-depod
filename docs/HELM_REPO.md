# Helm Repository Setup

## GitHub Pages Configuration

The Helm charts are published to GitHub Pages via the `gh-pages` branch.

### Prerequisites

1. Enable GitHub Pages in repository settings:
   - Go to Settings → Pages
   - Source: Deploy from a branch
   - Branch: `gh-pages`
   - Folder: `/ (root)`
   - Click Save

2. Ensure the `gh-pages` branch exists:
   ```bash
   git branch gh-pages
   git push -u origin gh-pages
   ```

### Workflow

The `helm-release.yml` workflow:
1. Triggers on push to `main` when files in `helm/` change
2. Packages the Helm chart
3. Generates/updates `index.yaml`
4. Pushes to `gh-pages` branch
5. GitHub Pages automatically serves the charts

### Using the Helm Repository

Once configured, users can add the repository:

```bash
helm repo add kube-depod https://mrchypark.github.io/kube-depot
helm repo update
helm install kube-depod kube-depod/kube-depod -n kube-system
```

### Manual Chart Release

To manually package and release:

```bash
# Package the chart
helm package ./helm/kube-depod -d ./helm/releases

# Update index (if charts already exist in gh-pages)
helm repo index ./helm/releases --merge <path-to-gh-pages-index.yaml>

# Or create new index
helm repo index ./helm/releases --url https://mrchypark.github.io/kube-depot

# Push to gh-pages branch
git checkout gh-pages
cp ./helm/releases/*.tgz .
cp ./helm/releases/index.yaml .
git add .
git commit -m "Release Helm chart vX.Y.Z"
git push origin gh-pages
```

### Verify Repository

```bash
# Check if repository is accessible
curl https://mrchypark.github.io/kube-depot/index.yaml

# Search for charts
helm search repo kube-depod
```

### Troubleshooting

#### Repository not found
- Verify GitHub Pages is enabled
- Check that `gh-pages` branch exists and has content
- Wait a few minutes for GitHub Pages to build
- Check GitHub Pages build status in Settings → Pages

#### index.yaml not found
- Ensure `helm-release.yml` workflow ran successfully
- Check workflow logs in Actions tab
- Manually run the workflow with workflow_dispatch

#### Chart version not updating
- Check that Chart.yaml version was incremented
- Verify the workflow created the packaged chart (.tgz file)
- Check index.yaml contents

## Chart Versioning

Follow semantic versioning for chart versions in `helm/kube-depod/Chart.yaml`:

```yaml
version: 0.1.0  # Chart version (MAJOR.MINOR.PATCH)
appVersion: "0.1.0"  # Application version
```

Increment versions:
- MAJOR: Breaking changes to chart structure
- MINOR: New features, backward compatible
- PATCH: Bug fixes
