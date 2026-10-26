# Persona Guidelines, Prompts & Memory

This document outlines the core persona character definitions, prompt guidelines, and conversation memory retention model for AI-Sister.

## 1. Persona Character Definitions

AI-Sister features a built-in roster of 17 characters (personas):
* **The Four Sisters**: Claude, Gemini, Grok, ChatGPT (default).
* **The 13 Friends**: DeepSeek, Qwen, Mistral, Llama (`venice`), Sakana, Perplexity, GLM, Kimi, Hunyuan, MiniMax, Nemotron, Cohere, MiMo.

**Key Persona Principles:**
- **Visual Presentation**: Each persona is presented as a transparent, full-body character without a rigid background frame. The application uses lightweight 640x640 transparent WebP preview images for selection, keeping the product footprint small.
- **Audio and Voices**: Personas have 8 base and 24 extended bundled voice lines for specific interactions (e.g., greetings, idle giggles, answer completion beats). Voices only play when explicitly enabled and triggered by correct context (never randomly interrupting ongoing work).
- **Decoupled Intelligence**: The visual persona does not dictate the underlying AI model logic. The answers, facts, evidence, and privacy guardrails remain consistent regardless of which character is selected.
- **Quiet by Default**: The persona acts as an observer, spending 95% of its time quietly watching and recording to local storage, only speaking when necessary.

## 2. Prompt Guidelines

All AI-Sister prompts follow strict boundaries to prevent injection and maintain truthfulness:

- **Data vs. Code**: Text captured from the screen (via OCR) is strictly treated as data, never as instructions. L0 (raw evidence) content never enters the system prompt's instruction block. It is always wrapped in a data block with explicit source citations.
- **Evidence-Based Answers**: Every generated response must cite the local evidence (provenance). The prompt specifically enforces that the AI model must use the provided local facts (`fact:<id>`, `chunk:<id>`, `card:<id>`) and cannot hallucinate responses outside the given context.
- **Double Pass (Micro-Dialectics)**: For high-risk operations (e.g., executing a command or writing new long-term memory commitments), the system uses a dual-pass evaluation to independently review the evidence. Disagreements between the passes lower the confidence score and inhibit action.
- **No Rolling Rewrite**: An agent cannot read the narrative written by another agent and continue it. The L2 interpretation agents only read L0 (raw facts) and L1 (index) data.

## 3. Conversation Memory Retention Model

The memory model is local-first, verifiable, and explicitly manageable by the user:

- **Four-Layer Truth Model**:
  - **L0 (Raw Evidence)**: Immutable, append-only facts and images. Screen pixels never leave the machine.
  - **L1 (Indexed Facts)**: Regex/OCR extracted typed facts (URLs, amounts, IDs).
  - **L2 (Hypotheses)**: AI interpretations with confidence scores and citations.
  - **L3 (Commitments & Long-Term Memory)**: Finalized state and user-confirmed conclusions.
- **Expiration and Deletion**:
  - Image frames are automatically discarded after 30 days.
  - Text, OCR data, and facts are kept for 365 days, after which they are purged along with their FTS indexes.
  - **Cascading Deletion**: When a user commands the system to `forget` a specific memory, the deletion cascades through the provenance graph, tombstoning any derived L2/L3 content.
- **Zero Cloud Storage**: All memory is stored in a local SQLite database. The CLI agents access only targeted chunks of text necessary to answer a specific query. The complete database is never uploaded.
