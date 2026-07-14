---
name: code-comment
description: Write, review, and improve Rust code comments and documentation. Use when adding comments, reviewing code quality, documenting Rust crates, APIs, unsafe code, concurrency, algorithms, or complex implementation decisions. Focus on explaining intent, invariants, safety requirements, constraints, and trade-offs rather than describing syntax.
---

# Rust Code Comment Quality

## Purpose

Comments and documentation exist to preserve knowledge that the compiler and code structure cannot express.

The primary question for every comment:

> "What important context would a future Rust developer lose if this comment disappeared?"

Good comments explain:

- why a design exists
- why an unusual implementation is necessary
- safety assumptions
- invariants
- ownership/lifetime decisions
- performance trade-offs
- external constraints
- business rules

Avoid comments that merely describe what the code already says.

---

# Rust Documentation Hierarchy

Use the correct documentation style.

## Crate / Module Documentation

Use `//!` for describing modules and crates.

Good:

```rust
//! Handles communication with the payment provider.
//!
//! Requests are batched because the provider enforces
//! strict rate limits.
```

Avoid:

```rust
//! This module contains functions for payments.
```

---

## Public API Documentation

Use `///` for public items.

Document:

- purpose
- behavior
- assumptions
- panics
- errors
- safety requirements
- examples when useful

Example:

```rust
/// Calculates the final invoice amount.
///
/// Discounts are applied before tax because this matches
/// the accounting system's calculation rules.
///
/// # Errors
///
/// Returns an error if the invoice currency is unsupported.
pub fn calculate_total(invoice: &Invoice) -> Result<Money, Error> {
    ...
}
```

---

# What Good Rust Comments Explain

## 1. Why, Not What

Bad:

```rust
// Increment counter
counter += 1;
```

Good:

```rust
// Increment before storing because downstream metrics assume
// the counter represents completed operations only.
counter += 1;
```

---

## 2. Invariants

Explain rules that must always remain true.

Example:

```rust
// The buffer length must never exceed capacity.
// Consumers rely on this invariant when reading without copying.
assert!(buffer.len() <= capacity);
```

---

## 3. Ownership and Lifetime Decisions

Rust code often contains deliberate ownership choices.

Example:

```rust
// Clone intentionally: the worker thread outlives the request handler,
// and borrowing would introduce unnecessary lifetime coupling.
let config = config.clone();
```

---

## 4. Performance Decisions

Example:

```rust
// Use Vec instead of HashSet because iteration order is meaningful
// and the collection size is expected to stay below 100 elements.
let entries: Vec<Entry> = entries;
```

---

## 5. External Constraints

Example:

```rust
// The API rejects requests faster than 10/sec.
// Keep this delay even though it appears unnecessary.
tokio::time::sleep(delay).await;
```

---

# Unsafe Rust Comments

`unsafe` code requires explicit justification.

Every unsafe block should explain:

1. Why unsafe is required.
2. What safety assumptions are being relied upon.
3. Why those assumptions are valid.

Bad:

```rust
unsafe {
    ptr.read()
}
```

Good:

```rust
// SAFETY:
// `ptr` comes from Box::into_raw and remains valid until this function
// returns. No mutable references exist while reading.
unsafe {
    ptr.read()
}
```

Never leave unexplained unsafe blocks.

---

# Concurrency Comments

Document synchronization assumptions.

Example:

```rust
// Mutex protects both fields because they must be updated atomically.
// Updating them separately can expose inconsistent state.
struct Cache {
    data: Mutex<State>,
}
```

---

# Algorithm Comments

For complex algorithms, explain the idea.

Bad:

```rust
// Iterate backwards
for item in items.iter().rev() {
```

Good:

```rust
// Iterate backwards because removing entries while walking forward
// would invalidate indexes and increase complexity.
for item in items.iter().rev() {
```

---

# When NOT To Comment

Do not comment obvious Rust.

Bad:

```rust
// Create a new String
let name = String::new();
```

Bad:

```rust
// Return the value
return value;
```

Bad:

```rust
// Check if option exists
if value.is_some() {
```

Rust already expresses this clearly.

---

# Prefer Better Rust Over More Comments

Before adding a comment, consider:

- Better naming?
- Smaller functions?
- Stronger types?
- A newtype?
- An enum instead of boolean flags?
- A helper function?

Example:

Instead of:

```rust
// true means user has completed verification
if user.flags & 4 != 0 {
```

Prefer:

```rust
if user.is_verified() {
```

---

# Placement Rules

## Above the code it explains

Good:

```rust
// Keep this cache warm because startup latency affects CLI usability.
initialize_cache();
```

Avoid placing comments far away from the related code.

---

## Avoid trailing comments

Avoid:

```rust
let timeout = 30; // timeout value
```

Use:

```rust
// Timeout matches the external service SLA.
let timeout = Duration::from_secs(30);
```

---

# TODO Rules

TODOs must describe actionable work.

Good:

```rust
// TODO: Replace polling with webhook support once provider exposes events.
```

Bad:

```rust
// TODO: Improve this.
```

---

# Comments Must Age Well

Before adding a comment ask:

- Will this still be true after refactoring?
- Does it describe a stable reason?
- Does it explain something future developers cannot infer?

Delete comments that are:

- outdated
- redundant
- misleading

A wrong comment is worse than no comment.

---

# Rust Documentation Checklist

For public APIs:

- [ ] Does the documentation explain purpose?
- [ ] Are errors documented?
- [ ] Are panics documented?
- [ ] Are safety requirements documented?
- [ ] Are important examples included?

For implementation comments:

- [ ] Does it explain why?
- [ ] Does it document assumptions?
- [ ] Does it explain surprising choices?
- [ ] Is it close to the relevant code?
- [ ] Will it remain true?

---

# Review Rules

When reviewing Rust comments:

Approve comments that:

- explain intent
- document invariants
- justify unsafe code
- explain ownership decisions
- capture external constraints
- explain non-obvious trade-offs

Reject comments that:

- narrate syntax
- repeat the code
- explain trivial operations
- are stale
- hide unclear design

The best Rust code is not heavily commented.

The best Rust code has comments only where human reasoning is required.