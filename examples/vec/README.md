# vec-demo Example

This example demonstrates the use of `std.vec` from the Miva standard library (version 0.1.4).

## What It Covers

The example showcases all major operations on `Vec[T]`:

- **Construction**: `new[T]()` and `with_capacity[T](cap)`
- **Push & Growth**: `push[T](ref v, x)` with automatic capacity doubling
- **Element Access**: `get[T]` (bounds-checked) and `get_unchecked[T]` (unchecked)
- **Mutation**: `set[T](ref v, i, x)` to write by index
- **Pop**: `pop[T](ref v)` removes and returns the last element
- **Querying**: `len[T]`, `capacity[T]`, `is_empty[T]`
- **Memory Management**: `clear[T]`, `free[T]`, `shrink_to_fit[T]`
- **Deep Copy**: `copy[T]` creates an independent allocation
- **Truncation**: `truncate[T](ref v, new_len)` reduces length without reallocating

## Running

```bash
cd examples/vec
miva run --release
```

## Expected Output

```
After with_capacity: len = 0
After with_capacity: cap = 3
After three pushes: len = 3
After three pushes: cap = 3
After fourth push: len = 4
After fourth push: cap = 6
Element at index 0: 10
Element at index 2 (unchecked): 30
After set[1] = 99, get[1]: 99
Popped: 40
Now len = 3
Empty? no
len/cap: [3, 6]
After clear: len = 0
After clear: cap = 6
After free: len = 0
After free: cap = 0
Before shrink_to_fit: cap = 8
After shrink_to_fit: cap = 5
Original copy len = 5
Copied vec     len = 5
After truncate to 7: len = 7

vec demo completed successfully.
```
