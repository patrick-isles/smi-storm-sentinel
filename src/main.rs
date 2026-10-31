//! Linux Ring -2 SMM (System Management Mode) SMI Storm & Stolen Time Sentinel
//!
//! Executes direct x86_64 `RDTSC` instructions to detect stolen time gaps caused by
//! Ring -2 BIOS firmware abducting the CPU into SMRAM without operating system awareness.

mod exporter;
mod interrupt_scanner;
mod tsc_sampler;

use exporter::SmiExporter;
use interrupt_scanner::InterruptScanner;
use tsc_sampler::TscSampler;

use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn format_ns(ns: u64) -> String {
    if ns >= 1_000_000_000 {
        format!("{:.3} s", ns as f64 / 1_000_000_000.0)
    } else if ns >= 1_000_000 {
        format!("{:.2} ms", ns as f64 / 1_000_000.0)
    } else if ns >= 1_000 {
        format!("{:.2} µs", ns as f64 / 1_000.0)
    } else {
        format!("{} ns", ns)
    }
}

fn print_usage(prog: &str) {
    println!(
        r#"{} - Linux Ring -2 SMM SMI Storm & Stolen Time Sentinel

USAGE:
    {} [OPTIONS]

OPTIONS:
    --port <PORT>          Prometheus HTTP exporter port (default: 9114)
    --threshold-us <US>    Stolen time detection threshold in microseconds (default: 100)
    --interval <SECS>      Terminal refresh interval in seconds (default: 1.0)
    --daemon               Run in headless daemon mode (no terminal TUI)
    -h, --help             Show this help message

PROMETHEUS ENDPOINT:
    http://0.0.0.0:9114/metrics
"#,
        prog, prog
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut port: u16 = 9114;
    let mut threshold_us: u64 = 100;
    let mut interval_secs: f64 = 1.0;
    let mut daemon_mode = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse().unwrap_or(9114);
                    i += 1;
                }
            }
            "--threshold-us" => {
                if i + 1 < args.len() {
                    threshold_us = args[i + 1].parse().unwrap_or(100);
                    i += 1;
                }
            }
            "--interval" => {
                if i + 1 < args.len() {
                    interval_secs = args[i + 1].parse().unwrap_or(1.0);
                    i += 1;
                }
            }
            "--daemon" => {
                daemon_mode = true;
            }
            "-h" | "--help" => {
                print_usage(&args[0]);
                return;
            }
            _ => {}
        }
        i += 1;
    }

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Initialize TSC Sampler (threshold in nanoseconds)
    let sampler = Arc::new(TscSampler::new(threshold_us.saturating_mul(1000)));
    sampler.start_sampling();

    // Start Prometheus Exporter
    SmiExporter::start_exporter(sampler.clone(), port);

    println!(
        "[smi-storm-sentinel] Initialized on 0.0.0.0:{}/metrics (PID: {})",
        port,
        std::process::id()
    );

    let mut prev_irq = InterruptScanner::read_interrupts();

    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_secs_f64(interval_secs));

        let tsc = sampler.snapshot();
        let irq = InterruptScanner::read_interrupts();

        let delta_nmi = irq.nmi_total.saturating_sub(prev_irq.nmi_total);
        let delta_tlb = irq.tlb_total.saturating_sub(prev_irq.tlb_total);
        let delta_res = irq.res_total.saturating_sub(prev_irq.res_total);
        let delta_cal = irq.cal_total.saturating_sub(prev_irq.cal_total);
        prev_irq = irq.clone();

        if !daemon_mode {
            // ANSI terminal refresh
            print!("\x1B[2J\x1B[1;1H");
            println!("===============================================================================");
            println!("   SMI-STORM-SENTINEL: Ring -2 SMM Firmware Latency & Time-Theft Auditor");
            println!("===============================================================================");
            println!(
                " TSC Frequency:  {:.3} GHz  | Cycle Latency: ~{:.2} ns | Detection Floor: {} µs",
                tsc.tsc_hz as f64 / 1_000_000_000.0,
                1_000_000_000.0 / tsc.tsc_hz as f64,
                threshold_us
            );
            println!("-------------------------------------------------------------------------------");

            let status_badge = if tsc.is_storm_active {
                "[CRITICAL: SMM GHOST ABDUCTION DETECTED - RING -2 STOLE TIME FROM CPU]"
            } else if tsc.abduction_events_total > 0 {
                "[WARNING: PREVIOUS SMM TIME GAPS RECORDED - FIRMWARE LATENCY CONFIRMED]"
            } else {
                "[OPTIMAL: NO SMM FIRMWARE TIME THEFT DETECTED - CONTINUOUS REALITY]"
            };
            println!(" SMM Firmware Status:        {}", status_badge);
            println!("-------------------------------------------------------------------------------");
            println!(" [RDTSC HARDWARE LATENCY & TIME-THEFT METRICS]");
            println!(
                " Cumulative Stolen Time:     {:>12} (Total time CPU was abducted into SMRAM)",
                format_ns(tsc.stolen_time_ns_total)
            );
            println!(
                " Abduction Events Detected:  {:>12} (Hardware gaps exceeding {} µs threshold)",
                tsc.abduction_events_total, threshold_us
            );
            println!(
                " Peak Single Execution Freeze: {:>10}",
                format_ns(tsc.max_stolen_time_ns)
            );
            println!(
                " Most Recent Stolen Gap:     {:>12}",
                format_ns(tsc.last_gap_ns)
            );
            println!(
                " Active Sampling Loop Jitter: {:>11}",
                format_ns(tsc.current_jitter_ns)
            );
            println!("-------------------------------------------------------------------------------");
            println!(" [HARDWARE INTERRUPT PULSE FORENSICS (/proc/interrupts)]");
            println!(
                " {:<22} {:<15} {:<15}",
                "INTERRUPT TYPE", "CUMULATIVE", "RATE (/sec)"
            );
            println!(" -----------------------------------------------------------------------------");
            println!(
                " Non-Maskable (NMI):    {:<15} +{}/s",
                irq.nmi_total, delta_nmi
            );
            println!(
                " TLB Shootdowns (MMU):  {:<15} +{}/s",
                irq.tlb_total, delta_tlb
            );
            println!(
                " Rescheduling IPIs:     {:<15} +{}/s",
                irq.res_total, delta_res
            );
            println!(
                " Function Call IPIs:    {:<15} +{}/s",
                irq.cal_total, delta_cal
            );
            println!(
                " Machine Checks (MCE):  {:<15} (Hardware CPU/Bus Parity Errors)",
                irq.mce_total
            );
            println!("===============================================================================");
            println!(" Prometheus Exporter: http://0.0.0.0:{}/metrics", port);
            println!(
                " Polling: {}s | PID: {} | Press Ctrl+C to shutdown",
                interval_secs,
                std::process::id()
            );
        } else if tsc.is_storm_active {
            eprintln!(
                "[CRITICAL ALERT] SMM Firmware Time Theft Active! Peak stolen gap: {} (Threshold: {} µs)",
                format_ns(tsc.max_stolen_time_ns),
                threshold_us
            );
        }
    }

    sampler.stop();
    println!("\n[smi-storm-sentinel] Received interrupt signal. Shutting down cleanly.");
}
