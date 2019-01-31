# HOCON.rs [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT) [![Build Status](https://travis-ci.org/mockersf/hocon.rs.svg?branch=master)](https://travis-ci.org/mockersf/hocon.rs) [![Coverage Status](https://coveralls.io/repos/github/mockersf/hocon.rs/badge.svg?branch=master)](https://coveralls.io/github/mockersf/hocon.rs?branch=master) [![Realease Doc](https://docs.rs/hocon/badge.svg)](https://docs.rs/hocon) [![Crate](https://img.shields.io/crates/v/hocon.svg)](https://crates.io/crates/hocon)

Parse HOCON configuration files in Rust

The API docs for the master branch are published [here](https://mockersf.github.io/hocon.rs/).

## Usage

```rust
let s = r#"{"a":5}"#;
let doc = Hocon::load_from_str(s).unwrap();

assert_eq!(doc["a"].as_i64().unwrap(), 5);
```

```rust
let s = r#"{"b":5, "b":10}"#;
let doc = Hocon::load_from_str(s).unwrap();

assert_eq!(doc["b"].as_i64().unwrap(), 10);
```

## Status

https://github.com/lightbend/config/blob/master/HOCON.md

- [x] parsing JSON
- [ ] comments
- [ ] omit root braces
- [x] key-value separator
- [ ] commas
- [ ] whitespace
- [x] duplicate keys and object merging
- [ ] unquoted strings
- [ ] multi-line strings
- [ ] value concatenation
- [ ] path expressions
- [ ] path as keys
- [ ] substitutions
- [ ] includes
- [ ] conversion of numerically-indexed objects to arrays
