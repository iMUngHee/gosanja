# Code Mapper

Code Mapper is a Rust code-indexing engine in the research and design stage. Start at `docs/research/README.md` for the evidence behind proposals. Architecture decisions belong in the architecture document once it exists.

## Editing and verification

- For changes under `docs/research/`, follow `.ai/rules/research.md`.
- Edit shared agent guidance in `.ai/PROJECT.md`. The root `AGENTS.md` points to it and `CLAUDE.md` imports the root file. Check the wiring with `cmp .ai/PROJECT.md AGENTS.md` and `cmp .ai/rules/research.md .claude/rules/research.md`.
- When the Rust workspace is introduced, take formatter, lint and test commands from its actual toolchain and manifests. For an indexing change, use a small fixture with an expected extracted fact and check the engine's output against it. Record the fixture, command and observed result before relying on a claimed capability.

## Versioning

Choose the release unit when the first distributable crate or binary exists. Use its Cargo manifest as the version source; if packages ship together, use the workspace package version. At that point define the public interface whose compatibility governs version bumps.
