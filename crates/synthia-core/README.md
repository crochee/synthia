# synthia-core

Common utilities for the Synthia AI Agent framework.

## Features

- **ID Generation**: ULID-based session, tool call, task, and message ID generation
- **Time Utilities**: Timestamp parsing and formatting with UTC timezone support
- **Path Resolution**: Workspace path resolution and validation
- **Tool Schema**: JSON schema generation for tool parameters

## Usage

```rust
use synthia_core::{generate_session_id, format_timestamp_utc, parse_timestamp};

let session_id = generate_session_id();
let formatted = format_timestamp_utc(session_id);
let parsed = parse_timestamp(&formatted)?;
```