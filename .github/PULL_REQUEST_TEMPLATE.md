## Summary
<!-- One sentence -->

## Changes
-
-

## QA Checklist (all required before merge)

### Automated (CI must pass)
- [ ] `cargo check` passes
- [ ] `cargo fmt` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo bench` shows no regression >5%
- [ ] `cargo audit` reports no vulnerabilities

### Manual (Reviewer)
- [ ] Code follows project conventions (thiserror, no unwrap)
- [ ] All public items have doc comments
- [ ] Error handling is exhaustive
- [ ] Unsafe blocks have SAFETY comment + proof (if any)
- [ ] New deps are justified in PR description
- [ ] Tests cover happy path + error path + edge case
- [ ] No dead code or commented-out blocks

### OpenCode Audit
- [ ] `opencode pr` output attached below
- [ ] All critical findings resolved
- [ ] All warnings addressed or justified

### Performance (if hot path touched)
- [ ] Memory: no new unbounded allocations
- [ ] Latency: no new blocking calls on async runtime
- [ ] Lock: no new Mutex in hot path (use RwLock/DashMap)

## Related Issues
Closes #
