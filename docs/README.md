# kube-depod Documentation

Complete documentation for the kube-depod Kubernetes pod deletion operator.

## Directory Structure

### 📐 [Architecture](./architecture/)
High-level design and architectural decisions.

- **[0001.md](./architecture/0001.md)** - Early design documentation
- **[RECONCILIATION_REFACTOR.md](./architecture/RECONCILIATION_REFACTOR.md)** - Controller framework refactoring (KDEPOD-FIX-RECON-001)

### 🎨 [Design](./design/)
Detailed technical design and implementation specifications.

- **[CEL_ENGINE_REDESIGN.md](./design/CEL_ENGINE_REDESIGN.md)** - CEL expression engine architecture and lock-free concurrent design (v0.1.2)

### 📖 [Guides](./guides/)
How-to guides and user documentation.

- **[CEL_EXPRESSIONS.md](./guides/CEL_EXPRESSIONS.md)** - Guide to writing CEL expressions for policies
- **[DEPLOYMENT.md](./guides/DEPLOYMENT.md)** - Deployment instructions and configuration
- **[HELM_REPO.md](./guides/HELM_REPO.md)** - Helm chart repository setup

### ✅ [Tasks](./tasks/)
Task completion reports and tracking.

- **[0001-todo.md](./tasks/0001-todo.md)** - Initial todo tracking
- **[KDEPOD-FIX-RECON-001.md](./tasks/KDEPOD-FIX-RECON-001.md)** - Reconciliation loop refactoring task completion report

---

## Quick Start

1. **New to kube-depod?** Start with [Deployment Guide](./guides/DEPLOYMENT.md)
2. **Need to write policies?** Check [CEL Expressions Guide](./guides/CEL_EXPRESSIONS.md)
3. **Understand the architecture?** Read [Architecture Overview](./architecture/RECONCILIATION_REFACTOR.md)
4. **Technical deep dive?** See [CEL Engine Design](./design/CEL_ENGINE_REDESIGN.md)

---

## Recent Updates

### v0.1.2 (November 16, 2025)
- ✅ CEL engine redesign for production use
- ✅ Full pod object injection in CEL context
- ✅ Concurrent evaluation (lock-free design)
- ✅ Reconciliation loop refactoring to Controller framework
- ✅ Eliminated 15-second periodic API list polling

See:
- [CEL Engine Redesign](./design/CEL_ENGINE_REDESIGN.md)
- [Reconciliation Refactoring](./architecture/RECONCILIATION_REFACTOR.md)

---

## Document Map by Use Case

### For Operators / DevOps
- [Deployment Guide](./guides/DEPLOYMENT.md)
- [Helm Repository](./guides/HELM_REPO.md)

### For Policy Authors
- [CEL Expressions Guide](./guides/CEL_EXPRESSIONS.md)
- [CEL Engine Design](./design/CEL_ENGINE_REDESIGN.md)

### For Contributors / Maintainers
- [Architecture Overview](./architecture/RECONCILIATION_REFACTOR.md)
- [CEL Engine Design](./design/CEL_ENGINE_REDESIGN.md)
- [Task: Reconciliation Refactoring](./tasks/KDEPOD-FIX-RECON-001.md)

---

## Document Categories

| Category | Purpose | Files |
|----------|---------|-------|
| **Architecture** | System design, high-level decisions | 2 |
| **Design** | Technical specifications, implementation details | 1 |
| **Guides** | How-to documentation, user guides | 3 |
| **Tasks** | Project tasks, completion reports | 2 |
| **Total** | | **8** |

---

## Key Features Documented

- ✅ CEL expression evaluation engine
- ✅ Builtin TTL-based deletion policies
- ✅ Concurrent pod evaluation
- ✅ Rate limiting and safety checks
- ✅ Kubernetes integration (1.20+)
- ✅ Prometheus metrics export
- ✅ Pod disruption budget awareness (eviction)

---

## Related Files

- `README.md` - Project overview and quick start
- `RELEASE_NOTES_*.md` - Version release notes
- `examples/` - Example policy configurations
- `src/` - Source code with inline documentation
- `tests/` - Test documentation and examples

---

Last Updated: November 16, 2025
