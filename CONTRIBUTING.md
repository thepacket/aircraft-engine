# Contributing

Thanks for your interest. This project is an educational simulator: the goal
is models that are transparent, documented and validated against published
data, not maximum fidelity at any cost.

## Ways to help

- **Validation data.** The most valuable contribution is a real flight log
  (QAR extract, engine monitor export, test-cell trace) with a note on its
  source and licence, plus a lesson that replays it. Please only contribute
  data you have the right to share.
- **Calibration.** If a published figure (POH, type certificate data sheet,
  manufacturer bulletin) disagrees with the model, open an issue with the
  source and the page. Fixes to the calibration knobs in the spec structs
  should come with an updated validation test.
- **New engines.** Follow the pattern in `crates/engine-core/src/any.rs`
  (spec + model + validation tests) and `web/src/kinds.ts` (dials, controls,
  faults, signals, readouts, lessons). See the README's Architecture section.
- **Lessons.** Scenario scripts in `web/public/scenarios/` are plain text and
  need no build; the syntax is in the README and the IDE's Help tab.
- **Documentation.** `docs/MODEL.md` is the single source for the equations
  and is rendered in the IDE's Model tab. Keep it in step with the code.

## Development

```bash
cargo test -p engine-core          # physics validation (native)
./scripts/build-wasm.sh            # wasm module into web/src/wasm
cd web && npm install && npm run dev
```

Before opening a pull request:

- `cargo test -p engine-core` passes (`--no-fail-fast` to see every suite).
- `cd web && npx tsc --noEmit && npm run build` passes.
- Any number quoted in `docs/MODEL.md` or the README that you changed has a
  test pinning it.
- Commit messages say what changed and why; reference the published source
  for calibration changes.

## Reporting problems

Use GitHub Issues for bugs and calibration disagreements, and GitHub
Discussions for questions, teaching ideas and show-and-tell. Please include
the engine kind, the scenario text (or share link) and what you expected.

## Licence

By contributing you agree that your contributions are licensed under the MIT
License that covers the project.
