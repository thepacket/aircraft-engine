//! Native command-line runner: executes a scenario script and writes a CSV log.
//!
//! Usage:
//!   engine-cli spec                      # print the default engine spec JSON
//!   engine-cli design                    # print the sized design point
//!   engine-cli run <script> [--rate HZ] [--spec spec.json] > log.csv
//!
//! Script lines: `at <seconds> <command> [value]`, e.g.
//!   at 0   starter on
//!   at 20  fuel on
//!   at 90  tla 1.0
//!   at 150 end

use engine_core::cycle::size_design_point;
use engine_core::{AnyEngine, AnySpec, EngineSpec};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("spec") => {
            let kind = args.get(1).map(|s| s.as_str()).unwrap_or("turbofan");
            match AnySpec::default_for(kind) {
                Some(s) => println!("{}", s.to_json()),
                None => { eprintln!("unknown engine kind {kind} (turbofan | piston | turboprop)"); std::process::exit(2) }
            }
        }
        Some("design") => {
            let d = size_design_point(&EngineSpec::cfm56_7b26());
            println!("{}", serde_json::to_string_pretty(&d).unwrap());
        }
        Some("run") => run(&args[1..]),
        Some("point") => point(&args[1..]),
        _ => {
            eprintln!("usage: engine-cli spec [kind] | design | point ... | run <script> [--engine kind] [--rate HZ] [--spec spec.json]");
            std::process::exit(2);
        }
    }
}

/// Steady-state cycle at one operating point: `point <n1 %> [alt m] [mach] [dISA K]`
fn point(args: &[String]) {
    use engine_core::atmosphere::{inlet, isa};
    use engine_core::cycle::{steady_point, CycleInput};
    use engine_core::spec::interp;
    let spec = EngineSpec::cfm56_7b26();
    let d = size_design_point(&spec);
    let n1: f64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(100.0);
    let alt: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let mach: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let disa: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let inl = inlet(isa(alt, disa), mach);
    let n1c = n1 / inl.theta.sqrt() / 100.0;
    let n2c = interp(&spec.spools.n2_vs_n1, n1c * 100.0) / 100.0;
    let r = steady_point(&spec, &inl, CycleInput::steady(n1c, n2c, 0.0, None), d.core_nozzle_area_m2);
    println!("N1 {:.1}% (N1c {:.1}%)  N2 {:.1}%  alt {:.0} m  M {:.2}  dISA {:+.0} K", n1, n1c * 100.0, n2c * 100.0 * inl.theta.sqrt(), alt, mach, disa);
    println!("thrust {:.0} N ({:.0} lbf)   fuel {:.3} kg/s ({:.0} lb/h)   TSFC {:.3} lb/lbf/h", r.net_thrust_n, r.net_thrust_n / 4.448222, r.wf_kg_s, r.wf_kg_s * 7936.64, r.tsfc * 3600.0 * 9.80665);
    println!("T4 {:.0} K   EGT {:.0} C   OPR {:.2}   FPR {:.3}   BPR {:.2}   W2 {:.1} kg/s   W25 {:.1} kg/s", r.t4_k, r.egt_k - 273.15, r.overall_pressure_ratio, r.fan_pressure_ratio, r.bypass_ratio, r.w2_kg_s, r.w25_kg_s);
    println!("HPT PR {:.2}  LPT PR {:.2}  core NPR {:.2} (choked {})  V9 {:.0} m/s  V19 {:.0} m/s  A9 req {:.3} m2", r.hpt_pressure_ratio, r.lpt_pressure_ratio, r.core_nozzle.npr, r.core_nozzle.choked, r.core_nozzle.exit_velocity_m_s, r.bypass_nozzle.exit_velocity_m_s, r.core_nozzle.required_area_m2);
    println!("eff: thermal {:.3} propulsive {:.3} overall {:.3}", r.thermal_efficiency, r.propulsive_efficiency, r.overall_efficiency);
    for s in &r.stations {
        println!("  {:>3} {:<30} Tt {:7.1} K  Pt {:8.1} kPa  W {:6.1} kg/s", s.id, s.label, s.tt_k, s.pt_pa / 1000.0, s.w_kg_s);
    }
}

fn run(args: &[String]) {
    let script_path = args.first().expect("script path");
    let mut rate = 10.0;
    let mut spec: Option<AnySpec> = None;
    let mut kind = "turbofan".to_string();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--rate" => { rate = args[i + 1].parse().expect("rate"); i += 2; }
            "--engine" => { kind = args[i + 1].clone(); i += 2; }
            "--spec" => {
                let text = std::fs::read_to_string(&args[i + 1]).expect("read spec");
                spec = Some(AnySpec::from_json(&text).expect("parse spec"));
                i += 2;
            }
            other => panic!("unknown argument {other}"),
        }
    }
    let spec = spec.unwrap_or_else(|| AnySpec::default_for(&kind).unwrap_or_else(|| panic!("unknown engine kind {kind}")));
    let script = std::fs::read_to_string(script_path).expect("read script");
    let mut events: Vec<(f64, String, Option<String>)> = Vec::new();
    for line in script.lines() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 || parts[0] != "at" {
            if parts.first() == Some(&"when") { eprintln!("note: conditional lines are only supported in the IDE: {line}"); }
            else { eprintln!("bad line: {line}"); }
            continue;
        }
        let t: f64 = parts[1].parse().expect("time");
        let rest = if parts.len() > 3 { Some(parts[3..].join(" ")) } else { None };
        events.push((t, parts[2].to_string(), rest));
    }
    events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let end = events.iter().find(|e| e.1 == "end").map(|e| e.0).unwrap_or_else(|| events.last().map(|e| e.0 + 10.0).unwrap_or(60.0));

    let mut e = AnyEngine::new(spec);
    e.logger().set_rate(rate);
    e.logger().start(0.0);
    let dt = 0.02;
    let mut next = 0;
    let mut env = [0.0, 0.0, 0.0, 0.4];
    while e.time() < end {
        while next < events.len() && events[next].0 <= e.time() + 1e-9 {
            let (_, cmd, val) = &events[next];
            apply(&mut e, cmd, val.as_deref(), &mut env);
            next += 1;
        }
        e.step(dt);
    }
    let csv = e.logger_ref().to_csv();
    std::io::stdout().write_all(csv.as_bytes()).unwrap();
}

fn apply(e: &mut AnyEngine, cmd: &str, val: Option<&str>, env: &mut [f64; 4]) {
    let on = matches!(val, Some("on") | Some("1") | Some("true") | Some("run"));
    let num = || val.and_then(|v| v.split_whitespace().next()).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    let b = |x: bool| if x { 1.0 } else { 0.0 };
    let r = match cmd {
        "starter" => e.set_control("starter", b(on)),
        "fuel" => match e.kind() {
            "turbofan" => e.set_control("fuel_lever", b(on)),
            "turboprop" => e.set_control("condition_lever", b(on)),
            _ => e.set_control("mixture", b(on)),
        },
        "tla" | "throttle" | "power" => e.set_control("tla", num()),
        "n1" => e.set_control("n1_demand", if val == Some("off") { -1.0 } else { num() }),
        "antiice" => e.set_control("anti_ice", b(on)),
        "pack" => e.set_control("pack_bleed", b(on)),
        "reverser" => e.set_control("reverser", b(on)),
        "ctl" => {
            let mut it = val.unwrap_or("").split_whitespace();
            let name = it.next().unwrap_or("");
            let v = it.next().unwrap_or("1");
            let x = match v { "on" | "true" | "run" => 1.0, "off" | "false" | "cutoff" => 0.0, _ => v.parse().unwrap_or(0.0) };
            e.set_control(name, x)
        }
        "alt" => { env[0] = num(); e.set_environment(env[0], env[1], env[2], env[3]); Ok(()) }
        "mach" => { env[1] = num(); e.set_environment(env[0], env[1], env[2], env[3]); Ok(()) }
        "isa" => { env[2] = num(); e.set_environment(env[0], env[1], env[2], env[3]); Ok(()) }
        "humidity" => { env[3] = num(); e.set_environment(env[0], env[1], env[2], env[3]); Ok(()) }
        "running" => { e.set_running(num()); Ok(()) }
        "fault" => {
            // fault <name> [value]   e.g. "fault ignition_fail on", "fault start_fuel_factor 1.6"
            let mut it = val.unwrap_or("").split_whitespace();
            let mut name = it.next().unwrap_or("").to_string();
            let v = it.next().unwrap_or("on");
            let json_v = match v { "on" | "1" | "true" => "true".to_string(), "off" | "0" | "false" => "false".to_string(), _ => v.to_string() };
            for (a, bname) in [("surge", "trigger_surge"), ("flameout", "trigger_flameout"), ("oil_leak", "oil_leak_qt_per_min"), ("compressor_damage", "compressor_damage_pct")] {
                if name == a { name = bname.to_string(); }
            }
            e.set_faults_json(&format!("{{\"{name}\": {json_v}}}"))
        }
        "bottle" => e.set_faults_json("{\"fire_bottle\": true}"),
        "end" | "log" | "note" | "speed" | "engine" => Ok(()),
        other => Err(format!("unknown command {other}")),
    };
    if let Err(m) = r { eprintln!("{m}"); }
}
