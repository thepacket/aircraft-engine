# Engine model documentation

This is the complete description of the physics in `crates/engine-core`. Every
equation here is implemented literally in the code, and every number that is
not a physical constant lives in the engine specification JSON shown in the
IDE.

Part I (sections 1–10) covers the turbofan, the CFM International CFM56-7B26.
Part II (sections 11–14) covers the shared propeller model, the Lycoming
O-360-A4M piston engine and the Pratt & Whitney Canada PT6A-114A turboprop.
Section 15 lists the validation tests that pin every number quoted here.

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
Wf,demand = Wf,steady(N1, N2) + K (N1,cmd − N1)        K = 0.012 kg/s per %
```

and is then clamped by the **acceleration schedule** and the **deceleration
schedule**, both expressed as fuel flow over burner static pressure, exactly
the ratio-units real hydromechanical and electronic controls use:

```
Wf,max = (Wf/Ps3)_accel(N2c) · Ps3          Wf,min = (Wf/Ps3)_decel(N2c) · Ps3
```

The accel limit protects the HPC from surge (too much fuel raises the burner
back-pressure); the decel limit protects against lean blow-out and can never
exceed the steady fuel flow. The fuel metering unit adds a 0.15 s lag. The
small governor gain matters: a slow lever advance keeps the fuel close to the
steady value, a slam goes straight to the accel limit. After a surge the
accel limit is re-armed over 4 s so the FADEC cannot slam back into a stalled
compressor. The IDE plots demand, limits and actual fuel flow so the effect
of the schedules is visible in every transient.

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
| Idle → 95 % thrust | 6.1 s | ≤ 5 s from flight idle (FAR 33.73) | from the lower ground idle |
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
- A running engine that falls more than 8 % below idle N2 (surge cycling,
  severe over-bleed) hands control back to the sub-idle start schedule; the
  real FADEC's sub-idle logic is more elaborate.

---

# Part II: propeller, piston engine and turboprop

## 11. Propeller (shared)

A single blade element at 75 % radius with an effective blade area, plus the
induced inflow from momentum theory:

```
u   = 2π n (0.375 D)               tangential speed at the element
φ   = atan((V + vi) / u)           inflow angle
α   = β − φ − α0                   angle of attack (α0 = −2°, cambered section)
Cl  = Cl_α α, clipped at Cl_max (1.35 positive, 0.9 negative)
Cd  = Cd0 + k Cl²
T   = ½ ρ (u² + (V+vi)²) A_b (Cl cos φ − Cd sin φ)
Q   = ½ ρ (u² + (V+vi)²) A_b (Cl sin φ + Cd cos φ) · 0.375 D
T   = 2 ρ A_disc (V + vi) vi       momentum theory, iterated for vi
```

Past the stall angle (14° positive, 10° negative) lift and drag blend to the
flat-plate values 2 sin α cos α and 2 sin² α over 8°. For reverse thrust at
forward speed the inflow is capped at −V/2 (vortex-ring boundary). The blade
area and pitch are calibrated to the aircraft data (static rpm and cruise rpm
for the fixed-pitch Archer propeller; static thrust and cruise efficiency for
the Caravan's constant-speed propeller).

## 12. Piston engine: Lycoming O-360-A4M

### 12.1 Induction

Air flow by speed-density, with volumetric efficiency falling at low manifold
pressure (residual exhaust gas):

```
ṁ_air = η_v (0.45 + 0.55 MAP/p) · V_d · rpm/120 · MAP / (R T_ind)
```

The throttle is an orifice: `p − MAP = x(θ) ṁ²`, with the effective area
shaped as `a_idle + (1 − a_idle) θ^1.8` and `x` fixed by two published points
(full-throttle loss at rated rpm, idle MAP at idle rpm). Solving the quadratic
gives MAP for any throttle and rpm. Carburettor heat raises the induction
temperature 35 K and adds a small restriction; carburettor ice (OAT −7…+21 °C,
humidity ≥ 55 %, worst at partial throttle) closes the throttle area by up to
75 % and melts with carb heat.

### 12.2 Mixture

A fixed carburettor jet meters fuel by venturi pressure drop, so the fuel-air
ratio rises as the air thins:

```
F/A = 0.088 · m(mixture) · √(ρ0 / ρ_carb)
```

`m` is the mixture-lever curve (1 at full rich, idle cutoff below 8 %). Peak
EGT sits at the stoichiometric 0.0667; the engine quits below 0.052.

### 12.3 Power

```
P_ind = ṁ_air · min(F/A, 0.0667) · LHV · η_i (0.55 + 0.45 MAP/p) · (cylinders firing / 4)
P_fric = (60 kPa + 35 Pa/rpm · rpm + (p − MAP)) · V_d · rpm/120
P_brake = P_ind − P_fric
```

η_i = 0.40 at full load reproduces 180 hp at 2 700 rpm. Single-magneto
running loses 7 % (slower flame); a fouled plug or a dead cylinder removes a
cylinder. Rotor dynamics: `J dω/dt = Q_engine + Q_starter − Q_prop` with the
propeller torque from section 11; the fixed-pitch propeller therefore sets
the rpm for every throttle and airspeed.

### 12.4 Temperatures, oil and starting

- EGT (°C) = 560 + 300·load − 7 000·(F/A − 0.0667) rich side, 9 000 on the
  lean side; 3 s thermocouple lag.
- CHT (°F) target = OAT + 230 + 200·load − 0.7·TAS(kt) with rich-mixture
  cooling; 90 s time constant.
- Oil pressure = 10 + 0.028·rpm psi, derated hot, boosted cold (relief valve
  at 115 psi); oil temperature target OAT + 100 + 70·load − 0.3·TAS °F.
- Starting: the starter cranks at ~170 rpm; the engine fires above 90 rpm
  with magnetos on, F/A above the lean limit and either a prime charge (2–3
  shots, lasting 8 s) or a warm engine (CHT > 200 °F). Six or more shots
  cause an induction fire.

Calibration (PA-28-181 POH): full-throttle static 2 350 rpm / 160 hp /
13.6 gal/h / EGT 1 250 °F; 8 000 ft full throttle 2 550 rpm, 72 % power,
10.6 gal/h leaned to best power; idle 740 rpm at 12 inHg; magneto drop 56 rpm
each; climb CHT 365 °F, cruise 300 °F.

## 13. Turboprop: Pratt & Whitney Canada PT6A-114A

### 13.1 Gas generator

Single-spool compressor (PR 9.2, η 0.80 with fall-off away from design),
reverse-flow combustor (4 % loss), compressor turbine driving the compressor
and accessories, then a free power turbine (PT):

```
P_comp = W cp_a (T3 − T2) + P_ext         ΔT_CT = P_comp / (η_m W4 cp_g)
P45 = P4 / π_CT(T4)                        CT pressure ratio from η_CT
ΔT_PT = η_PT η_Np · T45 · (1 − (P5/P45)^((γ−1)/γ))     P5 = 1.05 p0 (exhaust stubs)
P_shaft = η_gearbox · W45 cp_g ΔT_PT
```

ITT is T45, the inter-turbine temperature. η_Np reduces the PT efficiency
away from its design speed ratio (`1 − 0.6 (1 − Np/1900)²`).

### 13.2 Closure

The PT nozzle guide vanes fix T4 for a given Ng: `W45 √T45 / P45 = A45 φ`,
where φ is the flow function, 1 when choked and falling as
`((1 − π_PT^−k)/(1 − π_crit^−k))^0.08` when the PT pressure ratio is low (idle).
A45 is sized so that 100 % Ng gives 675 shp at sea level. Published idle,
takeoff and cruise points are reproduced: idle Ng 52 % / ITT 500 °C /
136 lb/h; takeoff ITT 684 °C / 424 lb/h / torque 1 866 ft·lb; 10 000 ft cruise
torque 1 390 ft·lb / 302 lb/h / prop efficiency 0.84.

### 13.3 Controls and propeller

- Power lever: −0.3 … 0 reverse (blade angle to −11°, Ng up to 88 %), 0 …
  0.12 beta range (flat pitch), 0.12 … 1 forward (Ng from idle to 100 %).
- Condition lever: cutoff / low idle 52 % / high idle 65 %.
- Prop lever: feather (86°) below 5 %, then 1 600 … 1 900 rpm governed. The
  governor moves the blade angle at 0.08°/rpm·s; below the power needed to
  reach the selected rpm the blades sit on the fine stop (6°) and Np follows
  the torque balance `J_p dω/dt = Q_PT − Q_prop − Q_fric`.
- Np overspeed governor: fuel topping above 2 000 rpm; the blades back off
  above 2 090 rpm.
- Fuel control: Ng governor with Wf/Ps3 accel (0.30 … 0.34 (kg/h)/kPa) and
  decel (0.14 … 0.16) schedules, as for the turbofan.

### 13.4 Start, failures, oil

Starter-generator motoring to 20 % Ng (12 % on a weak battery), light-off
above 12 % Ng within 2 s, start fuel schedule scaled to the idle fuel flow,
starter cutout at 50 %. Hot start (weak battery, rich schedule), hung start
(early cutout), wet start (no ignition), flameout and relight (Ng above 12 %,
below 20 000 ft), surge margin and surge, oil leak to seizure (90 s below
40 psi), fire and bottle, prop governor failure (frozen blade angle), ITT and
torque indication failures, chip detector light.

Limits (208B POH): torque 1 970 ft·lb, ITT 805 °C continuous / 865 transient
/ 1 090 start (2 s), Ng 101.6 %, Np 1 900 (2 090 transient), oil 85–105 psi
(40 idle), oil temperature 99 °C.

## 14. Known limitations, Part II

- **Propeller**: one blade element with an effective area stands in for the
  radial distribution; no tip losses, no compressibility beyond reporting the
  tip Mach number, no slipstream swirl. Reverse thrust at forward speed uses
  the vortex-ring inflow cap, which is a bound, not a measurement.
- **Piston**: no mixture distribution between cylinders (all cylinders see
  the same F/A), no detonation model, no induction ram, no mechanical mixture
  cutoff lag; CHT and oil temperature are single lumped masses; the
  carburettor-ice rate is a simple band model, not a dew-point calculation.
- **Turboprop**: the gas generator has no separate power balance for the
  compressor turbine (it follows the fuel-driven acceleration like the
  turbofan's LP spool); idle shaft power is low (about 8 hp, so the
  propeller idles near 600 rpm instead of the real ~1 000); no inlet screen
  or inertial-separator pressure loss beyond a small bleed; the exhaust-stub
  jet thrust is a momentum estimate.

## 15. Validation tests

Every number in this document is pinned by a test in `crates/engine-core/tests`:

| File | Tests | What they check |
|---|---|---|
| `validation.rs` | 29 | turbofan: design point, takeoff, idle, cruise, monotonicity, bleed, start, FAR 33.73 accel, decel, flat rating, reverser, shutdown, cruise stability, hot/wet/hung starts, weak starter, surge recovery and cycling, flameout and relight, windmill envelope, altitude relight limit, oil starvation, fire bottle, governor failure, EGT probe, logger, spec round trip |
| `piston.rs` | 11 | full-throttle static, rated power, 8 000 ft cruise and leaning, peak EGT and idle cutoff, idle, cold start, magneto check with fouled plug and dead mag, carburettor ice, fuel starvation and restart, climb and cruise temperatures, logger |
| `turboprop.rs` | 12 | sized design point, takeoff, low and high idle, cruise, torque vs ITT limits hot and high, prop governing and feather, reverse, ground start, weak-battery hot start, flameout and relight, governor failure overspeed, logger |

Three ignored tests (`tp_calib`, `prop_calib`) print steady points and search
propeller geometry; run them with `cargo test -- --ignored --nocapture` when
recalibrating.
