# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

---

Review is split into several files:
./REVIEW_SECURITY.md

When you have some issue fixed mark in in the corresponding file.

---

## Appendix — commands used to verify

```bash
cargo test -q                                    # 14/14 pass
bash tests/launcher.sh -q                        # 167/167 pass
cargo clippy --all-targets                       # 25 lib + 80 bin warnings
# Bug repros: see sections A1–A7 for exact bash snippets
```
