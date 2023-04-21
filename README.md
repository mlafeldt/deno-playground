# deno-limits

This repo explores the implementation of resource limits (max memory and max duration) of V8 isolates managed by Deno.

The code was inspired by https://github.com/supabase/edge-runtime

## CLI example

```console
cargo run --bin limits -- ./js/hello.js
cargo run --bin limits -- ./js/timeout.js
cargo run --bin limits -- ./js/oom.js
```

## Rocket server example

```console
cargo run --bin rocket

curl -XPOST http://127.0.0.1:8000/hello
curl -XPOST http://127.0.0.1:8000/timeout
curl -XPOST http://127.0.0.1:8000/oom
```
