# Ares

Ares ~~is~~ will be an IDE backend for developing containerized applications between a host and a target.

# Usage

For now, Ares can only build an OCI image from the `context` directory and push it to a local registry which is also managed by Ares:

```
cargo run -- --access unix --context ./context --debug-output . --project ares-test
```