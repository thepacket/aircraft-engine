//! Station-by-station thermodynamic cycle of a two-spool, separate-flow
//! high-bypass turbofan.
//!
//! Station numbering (SAE ARP 755):
//!   0  freestream            13 fan exit, bypass stream
//!   2  fan face              19 bypass nozzle exit
//!   25 booster exit / HPC in  3 HPC exit (combustor inlet)
//!   4  combustor exit / HPT in
//!   45 HPT exit / LPT in     5 LPT exit     9 core nozzle exit
//!
//! Off-design behaviour: the fan and core operating lines are expressed as
//! simple power laws of corrected spool speed (a stand-in for the proprietary
//! component maps). The turbine inlet temperature is **not** scheduled; it is
//! found by requiring that the gas leaving the LPT fits through the fixed core
//! nozzle (flow continuity), exactly as in the real engine, where the nozzle
//! area and the spool speeds together fix T4.

use crate::atmosphere::{Inlet, GAMMA_AIR, R_AIR};
use crate::spec::{interp, EngineSpec};
use serde::{Deserialize, Serialize};

pub const CP_AIR: f64 = 1004.5;
pub const CP_GAS: f64 = 1156.0;
pub const GAMMA_GAS: f64 = 1.333;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Station {
    pub id: String,
    pub label: String,
    pub tt_k: f64,
    pub pt_pa: f64,
    pub w_kg_s: f64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Nozzle {
    pub npr: f64,
    pub exit_velocity_m_s: f64,
    pub exit_static_pressure_pa: f64,
    pub exit_temperature_k: f64,
    pub required_area_m2: f64,
    pub choked: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CycleResult {
    pub stations: Vec<Station>,
    /// Customer bleed flow extracted at station 3 (kg/s)
    pub bleed_kg_s: f64,
    /// Booster flow dumped to the bypass by the variable bleed valves (kg/s)
    pub vbv_dump_kg_s: f64,
    /// Burner static pressure estimate Ps3 (kPa), the FADEC's Wf/Ps3 reference
    pub ps3_kpa: f64,
    /// Reverse thrust component (N, positive = rearward force on the aircraft)
    pub thrust_reverse_n: f64,
    pub w2_kg_s: f64,
    pub w13_kg_s: f64,
    pub w25_kg_s: f64,
    pub wf_kg_s: f64,
    pub w5_kg_s: f64,
    pub bypass_ratio: f64,
    pub fan_pressure_ratio: f64,
    pub booster_pressure_ratio: f64,
    pub hpc_pressure_ratio: f64,
    pub overall_pressure_ratio: f64,
    pub hpt_pressure_ratio: f64,
    pub lpt_pressure_ratio: f64,
    pub t4_k: f64,
    pub egt_k: f64,
    pub core_nozzle: Nozzle,
    pub bypass_nozzle: Nozzle,
    pub fan_power_w: f64,
    pub hpc_power_w: f64,
    pub hpt_power_w: f64,
    pub lpt_power_w: f64,
    pub thrust_bypass_n: f64,
    pub thrust_core_n: f64,
    pub ram_drag_n: f64,
    pub net_thrust_n: f64,
    /// kg/(N s)
    pub tsfc: f64,
    pub thermal_efficiency: f64,
    pub propulsive_efficiency: f64,
    pub overall_efficiency: f64,
    pub combustion: bool,
}

/// Nozzle expansion of a stream with total conditions (tt, pt) to ambient p0.
/// Returns exit conditions and the throat/exit area needed to pass `w`.
pub fn nozzle(w: f64, tt: f64, pt: f64, p0: f64, gamma: f64, cp: f64) -> Nozzle {
    let npr = pt / p0;
    let crit = ((gamma + 1.0) / 2.0).powf(gamma / (gamma - 1.0));
    if npr <= 1.0001 || w <= 0.0 || tt <= 0.0 {
        return Nozzle {
            npr,
            exit_velocity_m_s: 0.0,
            exit_static_pressure_pa: p0,
            exit_temperature_k: tt,
            required_area_m2: 1.0e6,
            choked: false,
        };
    }
    if npr < crit {
        let t_exit = tt * npr.powf(-(gamma - 1.0) / gamma);
        let v = (2.0 * cp * (tt - t_exit).max(0.0)).sqrt();
        let rho = p0 / (R_AIR * t_exit);
        Nozzle {
            npr,
            exit_velocity_m_s: v,
            exit_static_pressure_pa: p0,
            exit_temperature_k: t_exit,
            required_area_m2: w / (rho * v.max(1e-6)),
            choked: false,
        }
    } else {
        let t_exit = tt * 2.0 / (gamma + 1.0);
        let p_exit = pt * (2.0 / (gamma + 1.0)).powf(gamma / (gamma - 1.0));
        let v = (gamma * R_AIR * t_exit).sqrt();
        let rho = p_exit / (R_AIR * t_exit);
        Nozzle {
            npr,
            exit_velocity_m_s: v,
            exit_static_pressure_pa: p_exit,
            exit_temperature_k: t_exit,
            required_area_m2: w / (rho * v),
            choked: true,
        }
    }
}

fn compress(tt_in: f64, pr: f64, eta: f64) -> f64 {
    let k = (GAMMA_AIR - 1.0) / GAMMA_AIR;
    tt_in * (1.0 + (pr.powf(k) - 1.0) / eta)
}

/// Turbine pressure ratio for a given temperature drop and isentropic efficiency.
fn turbine_pr(tt_in: f64, dt: f64, eta: f64) -> f64 {
    let k = (GAMMA_GAS - 1.0) / GAMMA_GAS;
    let ratio_s = (1.0 - (dt / tt_in) / eta).max(0.02);
    ratio_s.powf(-1.0 / k)
}

/// Combustor energy balance: fuel flow required to raise `w` kg/s from t3 to t4.
pub fn fuel_for_t4(spec: &EngineSpec, w: f64, t3: f64, t4: f64) -> f64 {
    let d = &spec.design_point;
    let num = w * CP_GAS * (t4 - t3);
    let den = d.eta_burner * d.fuel_lhv_j_kg - CP_GAS * t4;
    (num / den).max(0.0)
}

/// Inverse of [`fuel_for_t4`].
pub fn t4_for_fuel(spec: &EngineSpec, w: f64, t3: f64, wf: f64) -> f64 {
    let d = &spec.design_point;
    (wf * d.eta_burner * d.fuel_lhv_j_kg + w * CP_GAS * t3) / ((w + wf) * CP_GAS)
}

/// Operating condition inputs to the cycle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CycleInput {
    /// Corrected fan speed, fraction of 100% (N1c / 100)
    pub n1c: f64,
    /// Corrected core speed, fraction of 100%
    pub n2c: f64,
    /// Turbine inlet total temperature (K)
    pub t4_k: f64,
    /// Core nozzle exit area (m^2). None = perfectly matched (design sizing).
    pub a9_m2: Option<f64>,
    /// Combustion on. When false the turbines extract no work (cold cranking,
    /// windmilling) and no fuel is burned.
    pub combustion: bool,
    /// Customer bleed (anti-ice + packs), fraction of core flow taken at station 3
    pub bleed_fraction: f64,
    /// Variable bleed valve position, 0 closed .. 1 open
    pub vbv_open: f64,
    /// Thrust reverser deployment, 0 stowed .. 1 deployed
    pub reverser: f64,
    /// Fraction of bypass thrust recovered as reverse thrust at full deployment
    pub reverser_efficiency: f64,
}

impl CycleInput {
    pub fn steady(n1c: f64, n2c: f64, t4_k: f64, a9_m2: Option<f64>) -> Self {
        CycleInput { n1c, n2c, t4_k, a9_m2, combustion: true, bleed_fraction: 0.0, vbv_open: 0.0, reverser: 0.0, reverser_efficiency: 0.0 }
    }
}

/// Compressor-side quantities that do not depend on T4 (used by the solvers).
pub struct ColdSection {
    /// Normalised speeds actually used (fan: n1c; core: n2c / n2c_design)
    pub n1: f64,
    pub n2: f64,
    pub w2: f64,
    pub w13: f64,
    pub w25: f64,
    pub fpr: f64,
    pub lpr: f64,
    pub hpr: f64,
    pub tt13: f64,
    pub tt25: f64,
    pub tt3: f64,
    pub pt13: f64,
    pub pt25: f64,
    pub pt3: f64,
}

pub fn cold_section(spec: &EngineSpec, inlet: &Inlet, n1c: f64, n2c: f64) -> ColdSection {
    let d = &spec.design_point;
    let n1 = n1c.max(0.0);
    // Core speed is referred to its design-point value (N2 at 100% N1), so
    // that the design pressure ratio and flow are reproduced at takeoff.
    let n2_design = interp(&spec.spools.n2_vs_n1, 100.0) / 100.0;
    let n2 = n2c.max(0.0) / n2_design;
    let corr = inlet.delta / inlet.theta.sqrt();
    let w25_d = d.mass_flow_kg_s / (1.0 + d.bypass_ratio);
    let mut w2 = d.mass_flow_kg_s * corr * n1.powf(d.fan_flow_exponent);
    let w25 = (w25_d * corr * n2.powf(d.core_flow_exponent)).max(1e-3);
    if w2 < 1.15 * w25 {
        w2 = 1.15 * w25;
    }
    let w13 = w2 - w25;
    let fpr = 1.0 + (d.fan_pressure_ratio - 1.0) * n1.powf(d.fan_pr_exponent);
    let lpr = 1.0 + (d.booster_pressure_ratio - 1.0) * n1.powf(d.fan_pr_exponent);
    let hpr = 1.0 + (d.hpc_pressure_ratio - 1.0) * n2.powf(d.hpc_pr_exponent);
    let fo = |eta: f64, n: f64| eta * (1.0 - d.compressor_efficiency_falloff * (1.0 - n.min(1.0)).powi(2));
    let tt13 = compress(inlet.tt2_k, fpr, fo(d.eta_fan, n1));
    let tt25 = compress(inlet.tt2_k, lpr, fo(d.eta_booster, n1));
    let tt3 = compress(tt25, hpr, fo(d.eta_hpc, n2));
    ColdSection {
        n1,
        n2,
        w2,
        w13,
        w25,
        fpr,
        lpr,
        hpr,
        tt13,
        tt25,
        tt3,
        pt13: inlet.pt2_pa * fpr,
        pt25: inlet.pt2_pa * lpr,
        pt3: inlet.pt2_pa * lpr * hpr,
    }
}

/// Full cycle evaluation at a given operating point.
pub fn cycle(spec: &EngineSpec, inlet: &Inlet, input: CycleInput) -> CycleResult {
    let d = &spec.design_point;
    let cs = cold_section(spec, inlet, input.n1c, input.n2c);
    let p0 = inlet.ambient.pressure_pa;
    let v0 = inlet.true_airspeed_m_s;

    // Customer bleed leaves at station 3; VBV dump leaves the booster for the bypass
    let fb = input.bleed_fraction.clamp(0.0, 0.3);
    let w_bleed = cs.w25 * fb;
    let w_vbv = cs.w25 * spec.bleed.vbv_dump_fraction * input.vbv_open.clamp(0.0, 1.0);

    // Combustor
    let fc = d.cooling_bleed_fraction;
    let w_burner = cs.w25 * (1.0 - fc - fb);
    let t4 = if input.combustion { input.t4_k.max(cs.tt3) } else { cs.tt3 };
    let wf = if input.combustion { fuel_for_t4(spec, w_burner, cs.tt3, t4) } else { 0.0 };
    let pt4 = cs.pt3 * (1.0 - d.burner_pressure_loss);
    let w4 = w_burner + wf;
    let w5 = cs.w25 * (1.0 - fb) + wf;

    // Shaft power balances (the HPC also compresses the bleed air; the
    // booster also compresses the VBV dump flow)
    let hpc_power = cs.w25 * CP_AIR * (cs.tt3 - cs.tt25) + d.power_extraction_w;
    let fan_power = cs.w13 * CP_AIR * (cs.tt13 - inlet.tt2_k) + (cs.w25 + w_vbv) * CP_AIR * (cs.tt25 - inlet.tt2_k);

    let (hpt_power, lpt_power) = if input.combustion {
        (hpc_power / d.eta_mechanical, fan_power / d.eta_mechanical)
    } else {
        (0.0, 0.0)
    };

    // HPT
    let dt_hpt = hpt_power / (w4 * CP_GAS);
    let tt45_uncooled = t4 - dt_hpt;
    let tf = |eta: f64, n: f64| eta * (1.0 - d.turbine_efficiency_falloff * (1.0 - n.min(1.0)).powi(2));
    let hpt_pr = if input.combustion { turbine_pr(t4, dt_hpt, tf(d.eta_hpt, cs.n2)) } else { 1.0 };
    let pt45 = pt4 / hpt_pr;
    // Cooling air returns downstream of the HPT
    let tt45 = (w4 * CP_GAS * tt45_uncooled + fc * cs.w25 * CP_AIR * cs.tt3) / (w5 * CP_GAS);

    // LPT
    let dt_lpt = lpt_power / (w5 * CP_GAS);
    let tt5 = tt45 - dt_lpt;
    let lpt_pr = if input.combustion { turbine_pr(tt45, dt_lpt, tf(d.eta_lpt, cs.n1)) } else { 1.0 };
    let pt5 = pt45 / lpt_pr;
    let egt = tt45 - d.egt_probe_lpt_fraction * dt_lpt;

    // Nozzles
    let core = nozzle(w5, tt5, pt5, p0, GAMMA_GAS, CP_GAS);
    let byp = nozzle(cs.w13, cs.tt13, cs.pt13, p0, GAMMA_AIR, CP_AIR);
    let a9 = input.a9_m2.unwrap_or(core.required_area_m2);
    let thrust_core = w5 * core.exit_velocity_m_s + (core.exit_static_pressure_pa - p0) * a9.min(core.required_area_m2 * 2.0);
    let thrust_byp = cs.w13 * byp.exit_velocity_m_s + (byp.exit_static_pressure_pa - p0) * byp.required_area_m2;
    let ram_drag = (cs.w2 + w_vbv) * v0;
    // Reverser: deployed blocker doors turn the bypass stream forward
    let rev = input.reverser.clamp(0.0, 1.0);
    let thrust_reverse = rev * input.reverser_efficiency * thrust_byp;
    let net = thrust_core + (1.0 - rev) * thrust_byp - thrust_reverse - ram_drag;
    let net = if rev > 0.0 { net } else { net.max(0.0) };

    // Efficiencies
    let ke_rate = 0.5 * (w5 * core.exit_velocity_m_s.powi(2) + cs.w13 * byp.exit_velocity_m_s.powi(2) - cs.w2 * v0 * v0);
    let net_for_eff = net.max(0.0);
    let fuel_power = wf * d.fuel_lhv_j_kg;
    let thermal = if fuel_power > 0.0 { (ke_rate / fuel_power).clamp(0.0, 1.0) } else { 0.0 };
    let propulsive = if ke_rate > 0.0 && v0 > 0.0 { (net_for_eff * v0 / ke_rate).clamp(0.0, 1.0) } else { 0.0 };
    let overall = if fuel_power > 0.0 { (net_for_eff * v0 / fuel_power).clamp(0.0, 1.0) } else { 0.0 };

    let st = |id: &str, label: &str, tt: f64, pt: f64, w: f64| Station { id: id.to_string(), label: label.to_string(), tt_k: tt, pt_pa: pt, w_kg_s: w };
    let stations = vec![
        st("0", "Freestream", inlet.ambient.temperature_k, p0, 0.0),
        st("2", "Fan face", inlet.tt2_k, inlet.pt2_pa, cs.w2),
        st("13", "Fan exit (bypass)", cs.tt13, cs.pt13, cs.w13),
        st("25", "Booster exit / HPC inlet", cs.tt25, cs.pt25, cs.w25),
        st("3", "HPC exit / combustor inlet", cs.tt3, cs.pt3, cs.w25),
        st("4", "Combustor exit / HPT inlet", t4, pt4, w4),
        st("45", "HPT exit / LPT inlet", tt45, pt45, w5),
        st("5", "LPT exit", tt5, pt5, w5),
        st("9", "Core nozzle exit", core.exit_temperature_k, core.exit_static_pressure_pa, w5),
        st("19", "Bypass nozzle exit", byp.exit_temperature_k, byp.exit_static_pressure_pa, cs.w13),
    ];

    CycleResult {
        stations,
        bleed_kg_s: w_bleed,
        vbv_dump_kg_s: w_vbv,
        ps3_kpa: cs.pt3 * 0.97 / 1000.0,
        thrust_reverse_n: thrust_reverse,
        w2_kg_s: cs.w2,
        w13_kg_s: cs.w13,
        w25_kg_s: cs.w25,
        wf_kg_s: wf,
        w5_kg_s: w5,
        bypass_ratio: cs.w13 / cs.w25,
        fan_pressure_ratio: cs.fpr,
        booster_pressure_ratio: cs.lpr,
        hpc_pressure_ratio: cs.hpr,
        overall_pressure_ratio: cs.lpr * cs.hpr,
        hpt_pressure_ratio: hpt_pr,
        lpt_pressure_ratio: lpt_pr,
        t4_k: t4,
        egt_k: egt,
        core_nozzle: core,
        bypass_nozzle: byp,
        fan_power_w: fan_power,
        hpc_power_w: hpc_power,
        hpt_power_w: hpt_power,
        lpt_power_w: lpt_power,
        thrust_bypass_n: thrust_byp,
        thrust_core_n: thrust_core,
        ram_drag_n: ram_drag,
        net_thrust_n: net,
        tsfc: if net > 1.0 { wf / net } else { 0.0 },
        thermal_efficiency: thermal,
        propulsive_efficiency: propulsive,
        overall_efficiency: overall,
        combustion: input.combustion,
    }
}

/// Find the turbine inlet temperature at which the LPT exit flow exactly fits
/// through the core nozzle of area `a9`. The required area decreases
/// monotonically with T4 (hotter gas -> lower turbine pressure ratios -> higher
/// nozzle pressure -> denser exit flow), so a bisection is sufficient.
/// `base` carries the operating point (speeds, bleed, VBV); its `t4_k` and
/// `a9_m2` are overridden.
pub fn solve_t4_for_nozzle(spec: &EngineSpec, inlet: &Inlet, base: CycleInput, a9: f64) -> f64 {
    let cs = cold_section(spec, inlet, base.n1c, base.n2c);
    let f = |t4: f64| {
        let r = cycle(spec, inlet, CycleInput { t4_k: t4, a9_m2: Some(a9), combustion: true, ..base });
        r.core_nozzle.required_area_m2 - a9
    };
    let mut lo = cs.tt3 + 20.0;
    let mut hi = 2600.0;
    if f(lo) <= 0.0 {
        return lo;
    }
    if f(hi) >= 0.0 {
        return hi;
    }
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if f(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 0.01 {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// Steady-state operating point at given corrected speeds (closure on the
/// core nozzle), including bleed and VBV effects.
pub fn steady_point(spec: &EngineSpec, inlet: &Inlet, base: CycleInput, a9: f64) -> CycleResult {
    let t4 = solve_t4_for_nozzle(spec, inlet, base, a9);
    cycle(spec, inlet, CycleInput { t4_k: t4, a9_m2: Some(a9), combustion: true, ..base })
}

/// Result of sizing the engine at its design point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignSizing {
    pub core_nozzle_area_m2: f64,
    pub bypass_nozzle_area_m2: f64,
    pub t4_takeoff_k: f64,
    pub takeoff: CycleResult,
}

/// Size the nozzles so that the engine produces its rated takeoff thrust at
/// sea level, static, ISA, 100% N1. Returns the derived nozzle areas and T4.
pub fn size_design_point(spec: &EngineSpec) -> DesignSizing {
    let inlet = crate::atmosphere::inlet(crate::atmosphere::isa(0.0, 0.0), 0.0);
    let n1c = 1.0;
    let n2c = interp(&spec.spools.n2_vs_n1, 100.0) / 100.0;
    let cs = cold_section(spec, &inlet, n1c, n2c);
    let thrust_at = |t4: f64| cycle(spec, &inlet, CycleInput::steady(n1c, n2c, t4, None)).net_thrust_n;
    let target = spec.rating.takeoff_thrust_n;
    let mut lo = cs.tt3 + 50.0;
    let mut hi = 2400.0;
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if thrust_at(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 0.005 {
            break;
        }
    }
    let t4 = 0.5 * (lo + hi);
    let r = cycle(spec, &inlet, CycleInput::steady(n1c, n2c, t4, None));
    DesignSizing {
        core_nozzle_area_m2: r.core_nozzle.required_area_m2,
        bypass_nozzle_area_m2: r.bypass_nozzle.required_area_m2,
        t4_takeoff_k: t4,
        takeoff: r,
    }
}
