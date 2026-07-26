# ADR-007: Shape Error Messages

**Status:** Proposed  
**Date:** 2026-07-26

## Context

Miva uses error codes like E0014 (type mismatch), E0016 (argument count), E0018 (unknown struct). Shape-related errors need clear messages.

## Decision

### New error codes

| Code | Context | Message |
|------|---------|---------|
| E0027 | Shape definition | `unknown shape 'Name'` |
| E0028 | Type annotation | `type '{expr}' does not satisfy shape '{name}': missing field '{field}'` |
| E0029 | Generic bound | `type '{resolved_type}' does not satisfy bound '{shape_name}'` |
| E0030 | Generic bound | `type '{resolved_type}' does not satisfy bound '{shape_name}': field '{field}' has type '{actual}' but expected '{expected}'` |

### Error message format

```
// Missing field
Error [E0028]: type 'Person' does not satisfy shape 'nameShape': missing field 'name'
  --> src/main.miva:15:14

// Type mismatch on field
Error [E0030]: type 'Person' does not satisfy bound 'hasValue': field 'value' has type 'string' but expected 'int'
  --> src/main.miva:22:18

// Unknown shape reference
Error [E0027]: unknown shape 'nonExistent'
  --> src/main.miva:10:12
```

## Rationale

- New error codes (E0027-E0030) avoid conflicts with existing codes
- Messages reference both the concrete type and the shape name for clarity
- Field-level detail helps users understand exactly which requirement failed
