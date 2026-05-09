# Optimus Programming Language

Optimus is a Rust-based language prototype that combines execution with built-in algorithm complexity reporting.

# Video Demonstration
https://drive.google.com/drive/folders/1zCtVD8gKINd1HPR-_Esw-K6PpvxEmTcN?usp=share_link

## What Is Complete

This prototype now includes a complete demonstration path with:

- Primitive data types: `int`, `float`, `bool`, `string`, `null`
- Mutability control: immutable-by-default variables, explicit `mut`
- Typed variables and assignments with runtime type checks
- Arithmetic and comparison expressions
- Control flow: `if / else if / else`, `while`, `for`
- Functions with typed parameters and optional typed return values
- `return` statements
- Classes with typed fields and methods
- Object construction with `new`
- Method calls and field access (`obj.method()`, `obj.field`)
- Modules and imports (`module`, `import`)
- Structured runtime errors (immutability/type/undefined symbol errors)
- Complexity analysis report at the end of execution
- Interactive REPL framework mode for live demos

## Quick Start

### Build

```bash
cargo build
```

### Run a Script

```bash
cargo run -- presentation.op
```

### Start GUI

```bash
cargo run -- server
```

Then open `http://127.0.0.1:7878` in your browser.

### Start REPL

```bash
cargo run -- repl
```

REPL commands:

- `:run` execute current buffer
- `:show` print current buffer
- `:clear` clear current buffer
- `:quit` exit REPL

## Presentation Scripts

- `presentation.op`: end-to-end full language showcase
- `test.op`: legacy stress test with nested loops and control flow
- `error_demo.op`: runtime error behavior example

## Language Examples

### Function

```optimus
fn sum_to(int n): int {
  mut int total = 0;
  for (mut int i = 1; i < n + 1; i = i + 1) {
    total = total + i;
  }
  return total;
}
```

### Class

```optimus
class Counter {
  mut int value = 0;

  fn init(int start): void {
    self.value = start;
    return;
  }

  fn inc(int by): int {
    self.value = self.value + by;
    return self.value;
  }
}
```

### Module

```optimus
module MathKit {
  fn square(int n): int {
    return n * n;
  }
}

import MathKit;
print(MathKit.square(12));
```

### Error Handling Example

```optimus
int immutable_value = 10;
immutable_value = 11;
```

Output includes:

```text
Runtime Error: cannot assign to immutable variable 'immutable_value'
```

## Complexity Report

Every script run ends with a Big-O summary based on detected loop nesting depth:

- `0` nested loops -> `O(1)`
- `1` nested loop -> `O(N)`
- `2` nested loops -> `O(N^2)`
- `3+` nested loops -> `O(N^X)`

## Project Structure

```text
optimus_compiler/
├── src/
│   ├── main.rs
│   ├── lexer.rs
│   ├── parser.rs
│   ├── ast.rs
│   └── analyzer.rs
├── presentation.op
├── error_demo.op
├── test.op
└── README.md
```
