//! International Standard Atmosphere (ISA) and inlet (ram) conditions.
//!
//! Valid from sea level to 20 km (troposphere + lower stratosphere).

use serde::{Deserialize, Serialize};

pub const T_SL: f64 = 288.15; // K
pub const P_SL: f64 = 101_325.0; // Pa
pub const R_AIR: f64 = 287.05; // J/(kg K)
pub const G0: f64 = 9.80665; // m/s^2
pub const LAPSE: f64 = -0.0065; // K/m, troposphere
pub const H_TROPOPAUSE: f64 = 11_000.0; // m
pub const GAMMA_AIR: f64 = 1.4;

/// Static ambient conditions at a given geopotential altitude.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Ambient {
    pub altitude_m: f64,
    pub temperature_k: f64,
    pub pressure_pa: f64,
    pub density_kg_m3: f64,
    pub speed_of_sound_m_s: f64,
}

/// ISA ambient conditions, with an optional temperature offset (ISA + delta).
pub fn isa(altitude_m: f64, delta_isa_k: f64) -> Ambient {
    let h = altitude_m.clamp(-1_000.0, 20_000.0);
    let (t_isa, p) = if h <= H_TROPOPAUSE {
        let t = T_SL + LAPSE * h;
        let p = P_SL * (t / T_SL).powf(-G0 / (LAPSE * R_AIR));
        (t, p)
    } else {
        let t11 = T_SL + LAPSE * H_TROPOPAUSE;
        let p11 = P_SL * (t11 / T_SL).powf(-G0 / (LAPSE * R_AIR));
        let p = p11 * (-G0 * (h - H_TROPOPAUSE) / (R_AIR * t11)).exp();
        (t11, p)
    };
    let t = t_isa + delta_isa_k;
    let rho = p / (R_AIR * t);
    Ambient {
        altitude_m: h,
        temperature_k: t,
        pressure_pa: p,
        density_kg_m3: rho,
        speed_of_sound_m_s: (GAMMA_AIR * R_AIR * t).sqrt(),
    }
}

/// Total (stagnation) conditions at the engine inlet face (station 2).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Inlet {
    pub ambient: Ambient,
    pub mach: f64,
    pub true_airspeed_m_s: f64,
    /// Total temperature at station 2 (K)
    pub tt2_k: f64,
    /// Total pressure at station 2 (Pa), including inlet ram recovery losses
    pub pt2_pa: f64,
    /// theta = Tt2 / 288.15
    pub theta: f64,
    /// delta = Pt2 / 101325
    pub delta: f64,
}

/// Inlet total conditions. Ram recovery follows a MIL-E-5008B style curve,
/// essentially 1.0 below Mach 1 for a subsonic pitot inlet (we use 0.995).
pub fn inlet(ambient: Ambient, mach: f64) -> Inlet {
    let m = mach.clamp(0.0, 0.95);
    let g = GAMMA_AIR;
    let tt2 = ambient.temperature_k * (1.0 + 0.5 * (g - 1.0) * m * m);
    let ram_recovery = 0.995;
    let pt2 = ambient.pressure_pa * (1.0 + 0.5 * (g - 1.0) * m * m).powf(g / (g - 1.0)) * ram_recovery;
    Inlet {
        ambient,
        mach: m,
        true_airspeed_m_s: m * ambient.speed_of_sound_m_s,
        tt2_k: tt2,
        pt2_pa: pt2,
        theta: tt2 / T_SL,
        delta: pt2 / P_SL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sea_level_is_standard() {
        let a = isa(0.0, 0.0);
        assert!((a.temperature_k - 288.15).abs() < 1e-9);
        assert!((a.pressure_pa - 101_325.0).abs() < 1e-6);
        assert!((a.density_kg_m3 - 1.225).abs() < 1e-3);
    }

    #[test]
    fn tropopause_matches_tables() {
        let a = isa(11_000.0, 0.0);
        assert!((a.temperature_k - 216.65).abs() < 0.01);
        assert!((a.pressure_pa - 22_632.0).abs() < 10.0);
    }

    #[test]
    fn fl350_matches_tables() {
        let a = isa(10_668.0, 0.0);
        assert!((a.pressure_pa - 23_842.0).abs() < 30.0);
    }
}
