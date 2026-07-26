# Glossary: Shape System

## Terms

**Shape**  
A structural contract defined by a set of named fields with types. A shape declares what fields a type must have but does not create a new type itself. Syntax: `shapeName = shape { field1: Type1, field2: Type2, }`.

**Shape satisfaction**  
The property that a concrete struct type has all the fields required by a shape, with compatible types. Satisfaction is superset-based: extra fields on the struct are allowed. Matching is order-independent and name-based.

**Shape bound**  
A constraint on a generic type parameter specifying which shapes the substituted type must satisfy. Syntax: `T: shapeName` or `T: shape1 + shape2` for multiple bounds.

**Generic shape**  
A shape that accepts type parameters, like `hasValue[T] = shape { value: T }`. The type parameters can be used in field types and instantiated with concrete types when used as bounds.

**First-class shape type**  
The ability to use a shape name as a type annotation directly: `let x: someShape = value`. The compiler verifies that the expression's type satisfies the shape.

**Structural subtyping**  
The type system principle that types are compatible based on their structure (fields) rather than their nominal identity (declaration). In Miva, a struct implicitly satisfies a shape if its fields are a superset — no explicit `implements` keyword needed.

**Compile-time erasure**  
Shapes produce zero runtime code. They exist only during compilation and are fully erased after type checking. All three backends (C++, LLVM, MVM) generate identical code regardless of whether shapes are used.

**Cross-module shape**  
A shape defined in one module and used in another. Exported via `export shapeName;` and resolved through the global shapes map built during Phase 0.5 of compilation.

## Relationships

```
Shape ──defines──> FieldSet { name: Type, age: int }
                      │
                      ├── satisfied by ──> Struct "Person" { name: string, age: int, email: string }
                      │
                      ├── bound to ──> GenericParam "T" in function signature
                      │
                      ├── referenced by ──> TypeAnnotation "let x: ShapeName"
                      │
                      └── exported via ──> SExport { symbol: "ShapeName" }
```

## Distinctions

- **Shape vs Struct**: A struct is a concrete type definition. A shape is a structural contract. They share syntax (field list) but serve different purposes. Defining a struct does NOT implicitly create a shape.
- **Shape vs Enum**: Enums are tagged unions (ADTs). Shapes are structural contracts. No relationship between them.
- **Shape vs Trait (Rust)**: Similar in concept (structural constraint), but Rust traits require explicit `impl Trait for Type` declarations. Miva shapes are implicit — any struct with matching fields automatically satisfies the shape.
- **Shape vs Interface (Go)**: Very similar. Both are structural and implicit. Key difference: Go interfaces can include method signatures; Miva shapes only describe data fields.
