# Alaco examples

Run an example from the repository root with `alaco run examples/json_demo.aco`.

- `json_demo.aco` — validate JSON and read top-level typed fields with fallbacks.
- `toml_demo.aco` — validate the supported TOML subset and read scalar settings.
- `http_demo.aco` — make an HTTPS GET request to `example.com`; requires network access.
- `type_safety.aco` — define a typed struct and use it in a function.

The HTTP example also contains a commented JSON POST example for a test endpoint. TOML support is intentionally a subset, not a full TOML 1.0 parser.
