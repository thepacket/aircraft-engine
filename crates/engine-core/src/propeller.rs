//! Propeller model shared by the piston and turboprop engines.
//!
//! A single blade element at 75 % radius with an effective blade area. It is
//! deliberately simple so that students can follow every term, yet it has the
//! right behaviour: thrust and torque rise with the square of rpm, efficiency
//! depends on advance ratio, the blade stalls at high angle of attack (static
//! runs), feather gives near-zero torque, and negative pitch gives reverse
//! thrust.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropSpec {
    pub diameter_m: f64,
    pub blades: u32,
    /// Effective blade area (all blades) used by the 75 % element, m^2.
    /// This is the main calibration knob (static rpm / static thrust).
    pub blade_area_m2: f64,
    /// Blade angle at 75 % radius for a fixed-pitch propeller (deg)
    pub fixed_pitch_deg: Option<f64>,
    /// Pitch range for a variable-pitch propeller (deg at 75 % radius)
    pub beta_min_deg: f64,
    pub beta_max_deg: f64,
    pub beta_feather_deg: f64,
    pub beta_reverse_deg: f64,
    /// Blade section aerodynamics
    pub cl_alpha_per_rad: f64,
    pub cl_max: f64,
    pub alpha_zero_lift_deg: f64,
    pub alpha_stall_deg: f64,
    /// Cambered sections stall earlier and lower at negative angle (reverse)
    pub alpha_stall_neg_deg: f64,
    pub cl_max_neg: f64,
    pub cd0: f64,
    pub k_induced: f64,
    /// Rotational inertia of the propeller (kg m^2), about its own shaft
    pub inertia_kg_m2: f64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PropForces {
    pub thrust_n: f64,
    pub torque_nm: f64,
    pub power_w: f64,
    pub beta_deg: f64,
    pub alpha_deg: f64,
    pub advance_ratio: f64,
    pub efficiency: f64,
    pub tip_mach: f64,
    pub stalled: bool,
}

/// Aerodynamic forces of a propeller at `rpm` (propeller shaft), blade angle
/// `beta_deg` at 75 % radius, true airspeed `v` (m/s), air density `rho`.
///
/// The blade element sees the freestream plus the induced inflow from
/// momentum theory (T = 2 rho A (V + vi) vi), iterated to convergence. Without
/// the inflow a static propeller would appear twice as effective as it is.
pub fn prop_forces(p: &PropSpec, rho: f64, speed_of_sound: f64, v: f64, rpm: f64, beta_deg: f64) -> PropForces {
    let n = rpm.abs() / 60.0; // rev/s
    let r = 0.375 * p.diameter_m; // 75 % radius
    let disc = std::f64::consts::PI * (0.5 * p.diameter_m).powi(2);
    let u = 2.0 * std::f64::consts::PI * n * r; // tangential speed at the element
    let mut vi = 0.0;
    let mut out = (0.0, 0.0, 0.0, false, 0.0);
    for _ in 0..16 {
        let ve = v + vi;
        let phi = ve.atan2(u.max(1e-6)); // inflow angle
        let alpha = beta_deg.to_radians() - phi - p.alpha_zero_lift_deg.to_radians();
        let alpha_stall = if alpha >= 0.0 { p.alpha_stall_deg } else { p.alpha_stall_neg_deg }.to_radians();
        let stalled = alpha.abs() > alpha_stall;
        let mut cl = (p.cl_alpha_per_rad * alpha).clamp(-p.cl_max_neg, p.cl_max);
        let mut cd = p.cd0 + p.k_induced * cl * cl;
        if stalled {
            // Post-stall: blend to flat-plate lift (2 sin a cos a) and drag (2 sin^2 a)
            let over = alpha.abs() - alpha_stall;
            let blend = (over / 0.14).min(1.0); // fully separated 8 deg past stall
            let cl_plate = alpha.signum() * 2.0 * alpha.abs().sin() * alpha.abs().cos();
            let cd_plate = 2.0 * alpha.abs().sin().powi(2) + p.cd0;
            cl = cl * (1.0 - blend) + cl_plate * blend;
            cd = cd * (1.0 - blend) + cd_plate * blend;
        }
        let q = 0.5 * rho * (u * u + ve * ve);
        let thrust = q * p.blade_area_m2 * (cl * phi.cos() - cd * phi.sin());
        let torque = q * p.blade_area_m2 * (cl * phi.sin() + cd * phi.cos()) * r;
        out = (thrust, torque, alpha, stalled, phi);
        // Momentum theory inflow for this thrust, relaxed
        let t_abs = thrust.abs();
        let mut vi_new = thrust.signum() * 0.5 * (-v + (v * v + 2.0 * t_abs / (rho * disc)).sqrt());
        // Reverse thrust at forward speed: momentum theory is invalid past the
        // vortex-ring boundary; cap the opposing inflow at half the airspeed.
        if thrust < 0.0 && v > 0.0 { vi_new = vi_new.max(-0.5 * v); }
        let vi_next = 0.5 * vi + 0.5 * vi_new;
        let converged = (vi_next - vi).abs() < 1e-3;
        vi = vi_next;
        if converged {
            break;
        }
    }
    let (thrust, torque, alpha, stalled, _phi) = out;
    let omega = 2.0 * std::f64::consts::PI * n;
    let power = torque * omega;
    let j = if n > 0.01 { v / (n * p.diameter_m) } else { 0.0 };
    let eff = if power > 1.0 && v > 0.0 { (thrust * v / power).clamp(0.0, 1.0) } else { 0.0 };
    let tip_speed = (2.0 * std::f64::consts::PI * n * 0.5 * p.diameter_m).hypot(v);
    PropForces {
        thrust_n: thrust,
        torque_nm: torque.max(0.0),
        power_w: power.max(0.0),
        beta_deg,
        alpha_deg: alpha.to_degrees(),
        advance_ratio: j,
        efficiency: eff,
        tip_mach: tip_speed / speed_of_sound,
        stalled,
    }
}

/// Constant-speed propeller governor: adjusts blade angle to hold the target
/// rpm. Returns the new blade angle. `on_fine_stop` is true when the governor
/// has run out of authority (blades on the fine-pitch stop, underspeed).
pub fn governor_step(p: &PropSpec, beta_deg: f64, rpm: f64, rpm_target: f64, gain_deg_per_rpm_s: f64, dt: f64) -> (f64, bool) {
    let err = rpm - rpm_target;
    let mut beta = beta_deg + gain_deg_per_rpm_s * err * dt;
    let on_fine_stop = beta <= p.beta_min_deg && err < 0.0;
    beta = beta.clamp(p.beta_min_deg, p.beta_max_deg);
    (beta, on_fine_stop)
}
