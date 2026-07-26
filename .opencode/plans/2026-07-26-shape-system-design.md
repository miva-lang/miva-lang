# ADR-001: Shape System Design

**Status:** Proposed  
**Date:** 2026-07-26  
**Subject:** Adding structural shape system to Miva

## Context

Miva currently has no way to constrain generic type parameters beyond bare names. All generics are unconstrained — any type can be substituted for `T`. We need a mechanism to require that a type has certain fields, analogous to Rust traits used as bounds and Go interfaces.

## Decision

Add `shape` as a third definition kind alongside `struct` and `enum`. Shapes define structural contracts: a set of named fields with types. Any struct whose fields are a superset of a shape's fields (same names, compatible types) implicitly satisfies that shape.

### Syntax

```miva
// Shape definition — struct-like field syntax with trailing commas
nameShape = shape {
  name: string,
  age: int,
}

// Generic shape with type parameter
hasValue[T] = shape {
  value: T,
}

// Using shape as first-class type annotation
let p: nameShape = person;

// Shape bounds on generic functions — Rust-style + syntax
greet[T: nameShape] = (p: T): string => p.name
multiBound[T: nameShape + ageShape] = (p: T) => ...

// Generic shape with bound containing generic type param
useValue[V: hasValue[int]] = (x: V) => x.value
```

### Key properties

- **Structural subtyping** — satisfaction is implicit, no explicit `implements` declaration needed. A struct with all required fields (and possibly more) satisfies the shape.
- **First-class type** — shapes can appear in type annotations (`let x: someShape = ...`). The compiler checks that the RHS type satisfies the shape.
- **Compile-time only** — shapes produce zero runtime code across all three backends (cxx, llvm, mvm). They are erased during type checking.
- **Order-independent** — field matching uses HashMap-based lookup, not positional order.
- **Superset OK** — extra fields on the struct do not prevent shape satisfaction.
- **Cross-module** — exported shapes are visible to other modules via the global shapes map.
- **Generic shapes** — shapes can have type parameters. Bounds can reference generic shape params.

### What this is NOT

- Not a new way to define structs (structs and shapes are separate concepts)
- Not runtime polymorphism (no vtables, no type erasure)
- Not duck typing at runtime (checked entirely at compile time)

## Consequences

- Adds a new AST node `DShape` and a new type variant `TShape`
- Requires a new `ShapeEntry` in the symbol table
- Extends generic function syntax with bounds parsing
- Type checker needs structural field comparison logic
- Three backends must skip shapes in codegen (no output)
- Cross-module resolution needs global shapes collection
