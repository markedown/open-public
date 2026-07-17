## Summary

<!-- What does this change do, and why? Keep it focused; unrelated changes belong in separate PRs. -->

## Related issues

<!-- e.g. Closes #123 -->

## Checklist

- [ ] Every new fact row references a `source_id` (including seed data and fixtures).
- [ ] No secrets, credentials, or infrastructure details are added. Only `.env.example` is tracked.
- [ ] Migrations are append-only: no already-applied migration was edited.
- [ ] Our own text (summaries, UI copy, comments, commit messages) stays neutral; fixtures use invented people.
- [ ] Time-varying facts are stored as time-ranged relations, not flat columns.
- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` pass locally.
- [ ] If any SQL query changed, `.sqlx/` was regenerated with `cargo sqlx prepare --workspace`.
