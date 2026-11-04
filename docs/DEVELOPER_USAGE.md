# Developer Usage

This document provides a brief overview of how to use the core components of `AI-Sister` for development.

## Core Interaction (`sister-core`)

The `sister-core` module is designed to be the central brain and data manager, entirely decoupled from platform-specific UI or capture logic.

When interacting with the core:
*   **Signals**: All events from the outside world (captures, user inputs, system events) must be translated into a `model::Signal`. The core processes these signals to update its L0-L3 states.
*   **Database**: The `Db` structure is your primary interface for persisting L0 evidence and reading back L1 facts or historical states.
*   **Models and Consent**: Any interaction that involves sending data to an LLM (L2 Hypotheses) goes through the `brain` module and the `provider_cli`. Importantly, any such call *must* be gated by `CloudAllowed` to ensure explicit user consent has been granted.

## Usage Tracking (`sister-usage`)

The `sister-usage` crate is responsible for tracking user quotas and usage limits. It operates independently to provide accurate local and public status updates.

*   **`LocalUsageAdapter`**: Use this adapter to read locally tracked usage data. It parses the local usage store and provides a structured view of current limits and consumption.
*   **`ConfiguredSessionAdapter`**: Use this adapter to understand the currently configured session details.

### Example: Reading Local Usage

```rust
// Conceptual usage, refer to `sister-usage` for exact traits
// let usage_adapter = LocalUsageAdapter::new(data_dir);
// match usage_adapter.read_usage() {
//     Ok(usage) => println!("Remaining quota: {}", usage.remaining),
//     Err(e) => eprintln!("Failed to read usage: {}", e),
// }
```

## Security and Privacy Guidelines

When developing new features or tools that integrate with `AI-Sister`:

1.  **Never bypass the `CloudAllowed` gate.** If your feature sends user data (OCR text, screenshots, etc.) to an external service or CLI, it must verify the `CloudAllowed` token.
2.  **Respect the Truth Model.** Do not write directly to L3 without proper review. Use `reviewer::L3Write` to ensure state changes are audited and adhere to the system's logic.
3.  **Local First.** Always default to local processing. External models should only be used for L2/L3 reasoning, not for L0/L1 fact gathering or basic extraction.
