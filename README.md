# Aircraft Engine IDE

An educational IDE for aircraft engine simulation. The simulators are
physics-based models built from the published specifications of real
engines, with virtual cockpit instrumentation and a data logger. Everything
runs inside the browser: the web server only delivers static files, and the
simulation core is Rust compiled to WebAssembly.

Default engine: **CFM International CFM56-7B26** (Boeing 737-800/900),
26,300 lbf takeoff thrust.

```
aircraft-engine/
├── crates/
│   ├── engine-core/      Rust simulation core (no I/O, runs natively and as wasm)
│   │   ├── src/atmosphere.rs   ISA atmosphere, inlet total conditions
│   │   ├── src/spec.rs         Engine specification (ratings, geometry, cycle, limits)
│   │   ├── src/cycle.rs        Station-by-station thermodynamic cycle, nozzles, sizing
│   │   ├── src/engine.rs       FADEC, spool dynamics, start/shutdown, sensors, limits
│   │   ├── src/logger.rs       Fixed-rate CSV data logger
│   │   └── tests/validation.rs Checks against published operating points
│   ├── engine-wasm/      wasm-bindgen bindings
│   └── engine-cli/       Native runner: `spec`, `design`, `point`, `run <script>`
├── web/                  TypeScript IDE (Vite). Static output in web/dist
│   ├── src/instruments.ts  EICAS-style SVG dials
│   ├── src/charts.ts       Strip charts
│   ├── src/scenario.ts     Scenario script runner
│   └── public/scenarios/   Lesson scripts
└── scripts/build-wasm.sh
```

## Build and run

Requirements: Rust (stable, with the `wasm32-unknown-unknown` target),
`wasm-bindgen-cli` matching the crate version pinned in
`crates/engine-wasm/Cargo.toml`, optionally `wasm-opt`, Node 20+.

```bash
# 1. Validate the physics natively
cargo test -p engine-core

# 2. Build the wasm module into web/src/wasm
./scripts/build-wasm.sh

# 3. Run the IDE (dev server with hot reload)
cd web && npm install && npm run dev

# 4. Production: static files only
cd web && npm run build      # -> web/dist
cd web && npm run preview    # or any static server, e.g. python3 -m http.server -d dist
```

Native tools without a browser:

```bash
cargo run -p engine-cli -- point 100            # takeoff point, SL static
cargo run -p engine-cli -- point 85 10668 0.78  # N1 85% at FL350 M0.78
cargo run -p engine-cli -- run web/public/scenarios/01-ground-start.txt > log.csv
```

## What the model reproduces

Published figures from the FAA type certificate data sheet, the 737NG FCOM
and CFM data are inputs: rated thrust, flat rating, fan diameter, mass flow,
bypass ratio, pressure ratios, N1/N2 at 100%, idle speeds, all EGT, speed and
oil limits, starter and start envelope. The validation suite checks that the
model lands on the published operating points:

| Point | Model | Published / expected |
|---|---|---|
| Takeoff thrust, SL ISA | 26,300 lbf (by sizing) | 26,300 lbf |
| Takeoff fuel flow | ~9,300 lb/h (TSFC 0.355) | ~9,000–10,000 lb/h |
| Takeoff EGT | ~857 °C | 800–900 °C (limit 950) |
| Overall pressure ratio | 32.7 | 32.7 |
| Ground idle N1 / N2 | 21 % / 60 % | 21 % / 60 % |
| Ground idle fuel flow, EGT | ~1,350 lb/h, ~450 °C | ~1,000–1,400 lb/h, 400–480 °C |
| Cruise FL350 M0.78, N1 85 % | ~4,200 lbf, TSFC 0.62 | 4,000–5,500 lbf, 0.60–0.65 |
| Idle to 95 % takeoff thrust | ~5–6 s | ≤ 5 s from flight idle (FAR 33.73) |
| Ground start, fuel-on to idle | ~32 s, EGT peak ~600 °C | 30–60 s, limit 725 °C |

## What is approximated

Component maps (fan, booster, HPC, turbines) and the FADEC control laws are
proprietary. They are replaced by:

- power-law component characteristics against corrected speed, calibrated to
  the published points above;
- a steady-state N2(N1) relation fitted to cockpit indications;
- turbine inlet temperature fixed by core-nozzle flow continuity (the same
  physical constraint that fixes it in the real engine);
- FADEC accel/decel schedules as N1 rate limits, with spool inertia providing
  the transient fuel and EGT overshoot.

The whole model is in the JSON spec shown in the IDE, so every assumption can
be inspected and changed.
