// Types mirrored from the Rust core (serde JSON).
export type Mode = "off" | "motoring" | "starting" | "running" | "shutdown";

export interface Station { id: string; label: string; tt_k: number; pt_pa: number; w_kg_s: number }
export interface Nozzle { npr: number; exit_velocity_m_s: number; exit_static_pressure_pa: number; exit_temperature_k: number; required_area_m2: number; choked: boolean }

export interface CycleResult {
  stations: Station[];
  bleed_kg_s: number;
  vbv_dump_kg_s: number;
  ps3_kpa: number;
  thrust_reverse_n: number;
  w2_kg_s: number; w13_kg_s: number; w25_kg_s: number; wf_kg_s: number; w5_kg_s: number;
  bypass_ratio: number; fan_pressure_ratio: number; booster_pressure_ratio: number; hpc_pressure_ratio: number; overall_pressure_ratio: number;
  hpt_pressure_ratio: number; lpt_pressure_ratio: number;
  t4_k: number; egt_k: number;
  core_nozzle: Nozzle; bypass_nozzle: Nozzle;
  fan_power_w: number; hpc_power_w: number; hpt_power_w: number; lpt_power_w: number;
  thrust_bypass_n: number; thrust_core_n: number; ram_drag_n: number; net_thrust_n: number;
  tsfc: number; thermal_efficiency: number; propulsive_efficiency: number; overall_efficiency: number; combustion: boolean;
}

export interface Controls { tla: number; fuel_lever: boolean; starter: boolean; anti_ice: boolean; pack_bleed: boolean; reverser: boolean; n1_demand_pct: number }
export interface Environment { altitude_m: number; mach: number; delta_isa_k: number }
export interface Faults {
  starter_inop: boolean; starter_weak: boolean; starter_early_cutout: boolean; ignition_fail: boolean; start_fuel_factor: number;
  fuel_pump_fail: boolean; oil_leak_qt_per_min: number; fire: boolean; n1_governor_fail: boolean; compressor_damage_pct: number;
  fod: boolean; egt_probe_fail: boolean; reverser_stuck: boolean; trigger_surge: boolean; trigger_flameout: boolean; fire_bottle: boolean;
}

export interface EngineState {
  time_s: number; mode: Mode; controls: Controls; environment: Environment; faults: Faults;
  inlet: { ambient: { altitude_m: number; temperature_k: number; pressure_pa: number; density_kg_m3: number; speed_of_sound_m_s: number }; mach: number; true_airspeed_m_s: number; tt2_k: number; pt2_pa: number; theta: number; delta: number };
  n1_pct: number; n2_pct: number; n1_rpm: number; n2_rpm: number; n1_corrected_pct: number; n2_corrected_pct: number;
  n1_command_pct: number; n1_limit_pct: number; n1_idle_pct: number; rating_limit: string; n1_dot_pct_s: number; n2_dot_pct_s: number;
  egt_c: number; egt_valid: boolean; egt_gas_c: number; t4_k: number; t4_steady_k: number;
  fuel_flow_kg_s: number; fuel_flow_pph: number; fuel_flow_steady_kg_s: number; fuel_command_kg_s: number; fuel_accel_limit_kg_s: number; fuel_decel_limit_kg_s: number;
  wf_p3_ratio: number; ps3_kpa: number; fuel_used_kg: number;
  thrust_n: number; thrust_lbf: number; thrust_reverse_n: number; tsfc_lb_lbf_h: number;
  oil_pressure_psi: number; oil_temperature_c: number; oil_quantity_qt: number; vib_n1: number; vib_n2: number;
  starter_engaged: boolean; ignition: boolean; lit: boolean; bleed_fraction: number; vbv_open: number; vsv_angle_deg: number; reverser_position: number;
  hpc_surge_margin_pct: number; fan_surge_margin_pct: number; surge: boolean; surge_count: number; flameout_count: number; fire_warning: boolean; seized: boolean; oil_starved_s: number; windmill_n2_pct: number;
  takeoff_timer_s: number; run_time_s: number; cycle: CycleResult; warnings: string[]; logger_rows: number; logger_enabled: boolean;
}

export interface EngineSpec {
  name: string; manufacturer: string; application: string;
  limits: { n1_max_pct: number; n2_max_pct: number; egt_takeoff_c: number; egt_max_continuous_c: number; egt_start_c: number; oil_pressure_min_psi: number; oil_pressure_caution_psi: number; oil_temp_max_continuous_c: number; oil_temp_max_transient_c: number; vib_advisory: number };
  oil: { capacity_qt: number; full_qt: number };
  design_point: { n1_100pct_rpm: number; n2_100pct_rpm: number };
  [k: string]: unknown;
}

export interface DesignSizing { core_nozzle_area_m2: number; bypass_nozzle_area_m2: number; t4_takeoff_k: number; takeoff: CycleResult }

export type EngineTarget = number | "all";
