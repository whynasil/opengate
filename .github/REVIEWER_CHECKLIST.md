# Code Review Checklist

## Every PR

### Correctness
- [ ] Logic is correct for all code paths (happy, error, edge)
- [ ] All errors are handled (no silent failures)
- [ ] No race conditions in async/concurrent code
- [ ] No `unwrap()`, `expect()`, bare `.index()`

### Safety
- [ ] Unsafe blocks have SAFETY comment + formal proof
- [ ] No memory leaks (Arc cycles, forgotten AbortHandle)
- [ ] File operations are sandboxed to workspace

### Performance
- [ ] No blocking calls in async context (std::fs, std::process::Command::output)
- [ ] Allocations are bounded (no unbounded Vec growth)
- [ ] Lock hold times < 1µs for Mutex, prefer RwLock/DashMap
- [ ] Channel senders use bounded channels where backpressure matters

### Testing
- [ ] New functionality has tests
- [ ] Tests cover error paths
- [ ] Test names describe behavior (`test_tool_timeout_kills_subprocess`)
- [ ] No flaky tests (tokio::time::advance where needed)

### Style
- [ ] `cargo fmt` and `cargo clippy` clean
- [ ] Doc comments on all `pub` items
- [ ] No commented-out code
- [ ] Imports grouped: std → external → crate

## Hot Path PRs (additional)

- [ ] Profile attached (flamegraph or criterion report)
- [ ] No regression vs main
- [ ] Allocations quantified

## After Review

- [ ] Approve: "LGTM, merge when CI green"
- [ ] Request changes: "See inline comments, please address and re-request"
- [ ] Comment: "Not blocking but consider..."
