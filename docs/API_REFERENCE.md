# API Reference

This document outlines the core APIs and structures available in `AI-Sister`, specifically focusing on `sister-core` and the platform-agnostic truth model.

## sister-core

`sister-core` acts as the platform-agnostic core of the application. It only understands signals (`model::Signal`) and is independent of specific OS implementations (Windows, macOS, or X11), which allows it to be fully tested on headless Linux for Phase 2 replay evaluation.

### The Four-Layer Truth Model (SPEC §0)

The core is structured around a four-layer truth model:

*   **L0 Evidence (L0 證據)**: Contains signal types from `model` and append-only tables from `db`. This layer bypasses LLMs entirely.
*   **L1 Facts (L1 事實)**: Typed extraction of facts via `facts`. Relies on pure rules, ensuring zero hallucinations and reproducibility.
*   **L2 Hypotheses (L2 假設)**: Managed by `brain`, which sends on-screen text (original, unredacted) to a user-configured CLI and receives a card. This requires explicit second-stage consent; otherwise, no text is sent.
*   **L3 State (L3 狀態)**: Managed exclusively by `reviewer`, which handles writing to promise tables, entities, and daily summaries. Writing to this layer explicitly requires a `reviewer::L3Write` instance.

### Core Exported Structures

The following are key structures and types exported by `sister-core` (as defined in `lib.rs`):

#### Configuration
*   **`Config`**: The main configuration wrapper.
*   **`BrainConfig`, `CaptureConfig`, `GatekeeperConfig`, `PrivacyConfig`, `RetentionConfig`**: Sub-configurations for specific features.
*   **`Exclusion`**: Defines exclusion rules.

#### Database & State
*   **`Db`**: The primary database interface.
*   **`DbStats`**: Database statistics.
*   **`SCHEMA_VERSION`**: Current schema version string.
*   **`FrameContext`**: Context associated with a captured frame.
*   **`FactRow`**: A database row representing a fact.

#### Models & Signals
*   **`Signal`**: The core event type understood by the system.
*   **`FrameCapture`**: Represents a captured screen frame.
*   **`HookHealth`**: Represents the health of input hooks.
*   **`FocusSnapshot`, `FocusEvent`, `FocusKind`**: Related to window focus state.
*   **`ClipboardEvent`, `ClipboardKind`**: Related to clipboard content.
*   **`InputListening`, `InputMetrics`, `InputTick`**: Related to input monitoring.
*   **`SystemEvent`, `SystemKind`**: System-level events.

#### Analysis & Extraction
*   **`ExtractedFact`**: Represents a fact extracted at the L1 layer.
*   **`FactKind`**: The type of extracted fact.
*   **`OcrBlock`, `TextChunk`**: Represents extracted text data.
*   **`SearchHit`**: A result from a search query.

#### Other Utilities
*   **`CloudAllowed`**: Consent structure ensuring models are only called when allowed.
*   **`Deduper`, `FrameVerdict`, `dhash_gray`, `dhash_rgb`, `hamming`**: Utilities for frame deduplication.
*   **`SecretKind`**: Enum for different types of detected secrets.
*   **`Millis`**: Time measurement in milliseconds.

## sister-usage

The `sister-usage` crate manages usage tracking, limits, and quotas.

### Exported Adapters

*   **`LocalUsageAdapter`**: Interface for reading local usage data.
*   **`ConfiguredSessionAdapter`**: Interface for accessing session configurations.
