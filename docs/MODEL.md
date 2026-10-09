# Engine model documentation

This is the complete description of the physics in `crates/engine-core`. Every
equation here is implemented literally in the code, and every number that is
not a physical constant lives in the engine specification JSON shown in the
IDE. The default engine is the CFM International CFM56-7B26.

## 1. Scope and philosophy

The model reproduces the **published** behaviour of the real engine: ratings,
speeds, limits, start envelope, acceleration times and the cockpit indications
at the main operating points. It replaces the **proprietary** parts, the
component maps and the FADEC control laws, with the simplest physically
meaningful stand-ins that can be calibrated to those published points.

The structure is the same as in a real performance deck:

- an **atmosphere and inlet** block gives total conditions at the fan face;
- a **steady-state cycle** gives every station temperature, pressure and flow
  for a given pair of spool speeds;
- a **control and dynamics** block (FADEC, spool inertia, start sequence,
  failures) moves the engine between those steady states in time;
- an **instrumentation** block adds sensor lags, noise and limit monitoring.

## 2. Atmosphere and inlet (station 0 → 2)

International Standard Atmosphere up to 20 km with an optional ISA deviation
ΔT. In the troposphere (h ≤ 11 000 m):

```
T(h) = 288.15 − 0.0065 h          [K]
p(h) = 101325 (T/288.15)^5.2559   [Pa]
```

Above the tropopause the temperature is held at 216.65 K and the pressure
decays exponentially. The ISA deviation is added to T after the pressure is
computed, as in the real atmosphere (pressure altitude is unchanged by
temperature).

Total conditions at the fan face for flight Mach number M, with γ = 1.4:

```
Tt2 = T (1 + 0.2 M²)
Pt2 = p (1 + 0.2 M²)^3.5 × 0.995      (0.995 = subsonic inlet pressure recovery)
θ = Tt2 / 288.15        δ = Pt2 / 101325
```

Corrected spool speeds are `N1c = N1 / √θ` and `N2c = N2 / √θ`. All component
characteristics are functions of corrected speed, which is why the engine
behaves differently at altitude for the same indicated N1.

## 3. Steady-state cycle (stations 2 → 9 and 13 → 19)

Station numbering follows SAE ARP 755: 2 fan face, 13 fan exit bypass, 25
booster exit, 3 HPC exit, 4 combustor exit, 45 HPT exit, 5 LPT exit, 9 core
nozzle exit, 19 bypass nozzle exit. Gas properties are constant per stream:
air cp = 1004.5 J/kg·K, γ = 1.4; combustion gas cp = 1156 J/kg·K, γ = 1.333.

### 3.1 Component characteristics (the stand-in for the maps)

With `n1 = N1c/100` and `n2 = N2c / N2c,design` (core speed referred to its
value at 100 % N1):

```
W2   = W2,d  · δ/√θ · n1^a_fan              total inlet flow
W25  = W25,d · δ/√θ · n2^a_core             core flow (W25,d = W2,d / (1 + BPR_d))
FPR  = 1 + (FPR_d − 1) · n1^b_fan           fan (bypass) pressure ratio
LPR  = 1 + (LPR_d − 1) · n1^b_fan           fan hub + booster pressure ratio
HPR  = 1 + (HPR_d − 1) · n2^b_hpc           HPC pressure ratio
η    = η_d · (1 − k (1 − n)²)               efficiency fall-off away from design
```

The exponents and fall-off factors are the calibration knobs (section 9). They
encode what a map would: flow and pressure rise grow with speed, efficiency
drops away from the design point.

### 3.2 Compression

For each compressor with pressure ratio π and isentropic efficiency η:

```
Tt,out = Tt,in · (1 + (π^((γ−1)/γ) − 1) / η)
Pt,out = Pt,in · π
```

applied 2→13 (fan, bypass stream), 2→25 (fan hub + booster) and 25→3 (HPC).

### 3.3 Bleeds and variable geometry

- **Turbine cooling** takes a fraction `f_c` (12 %) of W25 at station 3; it
  bypasses the combustor and the HPT and is mixed back in at station 45.
- **Customer bleed** (engine anti-ice 2.5 %, packs 4.5 %) is extracted at
  station 3 and leaves the engine: it has absorbed HPC work but produces no
  turbine work, which is why EGT rises and thrust drops when bleed is on.
- **Variable bleed valves** open below ~75 % N2c and dump up to 20 % of the
  booster flow into the bypass duct. The dumped air has absorbed booster work,
  so it adds to the LPT load at low speed.
- **Variable stator vanes** are scheduled against N2c for display only.

### 3.4 Combustor

Energy balance with burner efficiency η_b (0.995) and fuel heating value
LHV (43.0 MJ/kg), 5 % total pressure loss:

```
W_b = W25 (1 − f_c − f_bleed)
Wf  = W_b · cp_g (T4 − T3) / (η_b LHV − cp_g T4)
Pt4 = 0.95 Pt3
```

and its inverse `T4(Wf)` is used whenever the fuel flow is the known quantity
(start, transients).

### 3.5 Turbines

The HPT drives the HPC plus the accessory gearbox (150 kW); the LPT drives the
fan and booster (including the VBV dump flow). With mechanical efficiency η_m:

```
P_HPC = W25 cp_a (T3 − T25) + P_ext              ΔT_HPT = P_HPC / (η_m W4 cp_g)
P_fan = W13 cp_a (T13 − T2) + (W25 + W_vbv) cp_a (T25 − T2)
                                                  ΔT_LPT = P_fan / (η_m W5 cp_g)
```

Turbine pressure ratio from the temperature drop and isentropic efficiency:

```
π_t = (1 − (ΔT/Tt,in) / η_t)^(−γ/(γ−1))
```

The cooling air is mixed in after the HPT. The EGT probe of the CFM56-7B sits
at the LPT stage-2 nozzle, so the model reports `EGT = T45 − 0.15 ΔT_LPT`.

### 3.6 Nozzles and thrust

Each nozzle expands from (Tt, Pt) to ambient p0. With `NPR = Pt/p0` and the
critical ratio `((γ+1)/2)^(γ/(γ−1))`:

- unchoked (NPR below critical): full expansion, `V = √(2 cp (Tt − T_exit))`,
  exit pressure = p0;
- choked: sonic exit, `T_exit = 2Tt/(γ+1)`, `p_exit = Pt (2/(γ+1))^(γ/(γ−1))`,
  `V = √(γ R T_exit)`, pressure thrust `(p_exit − p0) A`.

```
F_net = W5 V9 + (p9 − p0) A9 + W13 V19 + (p19 − p0) A19 − W2 V0
```

The reverser turns a fraction (55 % at full deployment) of the bypass thrust
forward: `F_net = F_core + (1 − r) F_bypass − r·0.55·F_bypass − ram drag`.

### 3.7 Closure: what fixes T4

The equations above have one free variable for a given (N1, N2): the turbine
inlet temperature. In the real engine it is fixed by the fact that the gas
leaving the LPT has to fit through the fixed core nozzle. The model imposes
exactly that: the nozzle area required by the LPT exit flow,

```
A_req = W5 / (ρ9 V9)
```

decreases monotonically with T4 (hotter gas → lower turbine pressure ratios →
higher P5 → denser, faster exit flow), so a bisection finds the T4 at which
`A_req = A9`. Students can watch this in the IDE: the core nozzle area never
changes, the turbine inlet temperature follows the spool speeds.

### 3.8 Design-point sizing

The nozzle areas are **derived**, not typed. At sea level, static, ISA,
100 % N1, the model searches the T4 that gives the rated takeoff thrust and
records the resulting A9 and A19. Changing the rated thrust in the spec
therefore re-sizes the engine.

## 4. Control: the FADEC

### 4.1 N1 command

The thrust lever angle (0 … 1) is mapped onto corrected N1:

```
N1c,cmd = N1c,idle + (N1c,max − N1c,idle) · TLA^1.3
```

- **Idle schedule**: 21 % on the ground; 27 % + 1.2 %/km once airborne (Mach
  > 0.15); +4 % with engine anti-ice on.
- **Takeoff rating**: 100 % corrected N1, capped by the 104 % actual-speed
  limit.
- **Flat rating**: up to ISA + 15 K the rating is a constant corrected N1.
  Above the corner point the FADEC holds the **rated EGT** (the EGT reached at
  ISA + 15, sea level, 100 % N1) by reducing N1, found by bisection on the
  steady cycle. The IDE shows which limit is active ("N1" or "EGT").
- **Reverse**: N1 limited to 85 % with the reverser deployed.
- **Direct N1 demand** (autothrottle / flight-log replay) bypasses the lever.

### 4.2 Fuel governing and the Wf/Ps3 schedules

The governor is proportional on N1 error with a feed-forward of the steady
fuel flow for the current speeds:

```
Wf,demand = Wf,steady(N1, N2) + K (N1,cmd − N1)        K = 0.03 kg/s per %
```

and is then clamped by the **acceleration schedule** and the **deceleration
schedule**, both expressed as fuel flow over burner static pressure, exactly
the ratio-units real hydromechanical and electronic controls use:

```
Wf,max = (Wf/Ps3)_accel(N2c) · Ps3          Wf,min = (Wf/Ps3)_decel(N2c) · Ps3
```

The accel limit protects the HPC from surge (too much fuel raises the burner
back-pressure); the decel limit protects against lean blow-out. The fuel
metering unit adds a 0.15 s lag. The IDE plots demand, limits and actual fuel
flow so the effect of the schedules is visible in every transient.

## 5. Spool dynamics

With the actual fuel flow known, `T4 = T4(Wf)` from the combustor balance.
With choked turbines the turbine power at fixed speeds scales with T4, so
the surplus power on the LP spool is

```
P_excess = P_LPT,steady · (T4 / T4,steady − 1)
J1 ω1 dω1/dt = P_excess
```

with J1 = 40 kg·m² for the fan, booster, LPT and shaft. The HP spool is
lighter and follows its steady operating line `N2 = f(N1)` with a 0.6 s lag
that shortens when the engine is over-fuelled. The N2(N1) line is a fit to
cockpit indications (idle 21/60, climb 85/92, takeoff 100/97.5).

EGT during a transient comes from the actual T4, which is why an
acceleration shows an EGT overshoot that decays as the spools catch up.

## 6. Start, shutdown, windmilling and relight

- **Motoring**: starter torque is maximum at rest and falls linearly to zero
  at 2.33 × the maximum motoring speed (28 % N2 → zero torque at 65 %). Rotor
  drag is `(N2 − N2,windmill)/12 %/s`. The balance gives the 28 % maximum
  motoring speed and a weak starter (18 % maximum) cannot reach the 20 %
  light-off speed.
- **Light-off**: 2 s after fuel-on, if N2 ≥ 20 %, altitude ≤ 30 000 ft and
  the igniters work. Fuel follows the sub-idle schedule, scaled so that it
  meets the governed idle fuel flow (continuity into the governor).
- **Sub-idle acceleration**: gross turbine acceleration from a table vs N2,
  minus rotor drag, plus starter assist until cutout at 56 %. A **hung
  start** is the point where the turbine curve meets the drag line below idle
  (e.g. starter cut-out at 30 %). A **hot start** is a rich schedule; a
  **wet start** is fuel with no ignition. The flame is lost if N2 falls 3 %
  below the light-off speed.
- **Shutdown**: N2 decays with an 18 s time constant, N1 with 45 s, both to
  the windmilling floor in flight: `N2,wm = 60 M^1.2 %`, `N1,wm = 40 M %`.
- **Flameout** (fuel supply loss, or fuel below half the minimum): the FADEC
  turns the igniters on and relights automatically when the windmilling core
  is above 20 % and the aircraft is below 30 000 ft.

## 7. Surge margin, failures, oil and fire

- **HPC surge margin**: the available margin is 22 % at design, shrinking at
  low speed (`× (0.55 + 0.45 n2)`), minus any injected compressor damage.
  Over-fuelling raises the burner pressure roughly as `(T4/T4,steady)^0.3`
  once the HPC flow has readjusted along its speed line, so the remaining
  margin is `(1 + SM)/(T4/T4,ss)^0.3 − 1`. When it reaches zero the engine
  surges: thrust falls to 30 %, EGT jumps 150 K, N1 and N2 drop, the FADEC
  pulls fuel to the decel floor for 1.5 s, then recovers.
- **Oil**: pressure `0.78 psi/%N2 − 2 psi`, derated 0.12 psi/K above 90 °C oil
  temperature; temperature first-order toward ambient + 70 K at idle, + 100 K
  at takeoff (150 s). Quantity drops by the 1.5 qt running "gulp" and by any
  leak; below 3 qt the pressure falls proportionally. 90 s below the 13 psi
  minimum seizes the bearings: N2 decays with a 4 s time constant and the
  engine will not restart.
- **Fire**: raises oil temperature and vibration; the bottle only extinguishes
  it once the fuel lever is at CUTOFF.
- Other faults: governor channel loss (schedule runs 8 % past the limit), EGT
  harness open (indication blank, gas still hot), reverser stuck, FOD
  (vibration, 12 points of fan margin, +20 K EGT).

## 8. Sensors and indications

- EGT thermocouples: 1.5 s lag on top of a 0.35 s gas-path lag.
- Fuel flow indication: 0.4 s lag.
- Oil pressure: ±0.15 psi noise; vibration: 0.3 + 1.1 (N1/100)² units plus
  fault contributions and noise, clipped at 5.
- Limits monitored: EGT 950 °C takeoff (5 min) / 925 °C continuous / 725 °C
  start, N1 104 %, N2 105 %, oil pressure 13 psi minimum, oil temperature
  155 °C, surge margin below 5 %.

## 9. Calibration and expected error

Published sources: FAA Type Certificate Data Sheet E00055EN (ratings, flat
rating, speeds, EGT limits), Boeing 737NG FCOM limitations and engine
indications (idle, oil, vibration, start limits), CFM published data (mass
flow, bypass ratio, OPR, fan diameter), FAR 33.73 (acceleration time).

| Point | Model | Published / expected | Notes |
|---|---|---|---|
| Takeoff thrust, SL ISA | 26 300 lbf | 26 300 lbf | by sizing |
| Takeoff fuel flow / TSFC | 9 335 lb/h / 0.355 | ~9 000–10 000 / 0.35–0.38 | |
| Takeoff EGT | 857 °C | 800–900 °C, limit 950 | new-engine margin ~90 °C |
| T4 takeoff | 1 698 K | 1 600–1 750 K class | not published |
| OPR / BPR | 32.7 / 5.1 | 32.7 / 5.1 | |
| Ground idle N1 / N2 | 21 / 60 % | 21 / 60 % | by schedule |
| Ground idle fuel / EGT | 1 357 lb/h / 454 °C | ~1 000–1 400 / 400–480 | |
| Cruise FL350 M0.78 N1 85 % | 4 150 lbf / TSFC 0.62 | 4 000–5 500 / 0.60–0.65 | |
| Idle → 95 % thrust | 4.3 s | ≤ 5 s (FAR 33.73) | from ground idle |
| Takeoff → idle | ~10 s | 5–10 s | decel schedule |
| Ground start, fuel-on → idle | 27 s, EGT peak ~500 °C | 30–60 s, limit 725 °C | |
| Flat rating | EGT held above ISA+15 | TCDS: flat rated to 30 °C SL | |

Calibration knobs (all in the spec): flow and pressure-ratio exponents,
efficiency fall-off, cooling fraction, the N2(N1) line, the Wf/Ps3 schedules,
spool inertia, the start tables.

## 10. Known limitations

- No real component maps: surge lines, choke and the detailed shape of the
  operating lines are approximations; the HP spool does not have its own
  power balance.
- No inlet distortion, crosswind or fan flutter; no thermal growth and tip
  clearance effects on efficiency.
- No thrust-reverser aerodynamics beyond a fixed efficiency; no reverser
  re-ingestion.
- No fuel temperature, fuel-oil heat exchanger or IDG/hydraulic loads beyond
  a fixed accessory power.
- Steady-state accuracy is within a few percent at the calibrated points and
  degrades away from them; transients reproduce the published times and the
  qualitative shape of the published traces, not proprietary test-cell data.
