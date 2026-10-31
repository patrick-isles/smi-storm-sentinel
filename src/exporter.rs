//! Prometheus Metrics Exporter on :9114/metrics
//!
//! Exposes Ring -2 SMM stolen time, RDTSC hardware latency gaps,
//! and low-level kernel interrupt counters (NMI, TLB shootdowns, MCE).

use crate::interrupt_scanner::InterruptScanner;
use crate::tsc_sampler::TscSampler;
use std::sync::Arc;
use std::thread;
use tiny_http::{Response, Server};

pub struct SmiExporter;

impl SmiExporter {
    pub fn start_exporter(sampler: Arc<TscSampler>, port: u16) {
        thread::spawn(move || {
            let addr = format!("0.0.0.0:{}", port);
            let server = match Server::http(&addr) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Prometheus HTTP server failed to bind to {}: {}", addr, e);
                    return;
                }
            };

            for request in server.incoming_requests() {
                if request.url() == "/metrics" || request.url() == "/" {
                    let tsc = sampler.snapshot();
                    let irq = InterruptScanner::read_interrupts();

                    let mut body = String::with_capacity(4096);

                    body.push_str("# HELP smi_detection_threshold_nanoseconds Configured threshold for detecting hardware stolen time\n");
                    body.push_str("# TYPE smi_detection_threshold_nanoseconds gauge\n");
                    body.push_str(&format!("smi_detection_threshold_nanoseconds {}\n", tsc.threshold_ns));

                    body.push_str("# HELP smi_stolen_time_nanoseconds_total Cumulative time stolen from operating system by Ring -2 SMM\n");
                    body.push_str("# TYPE smi_stolen_time_nanoseconds_total counter\n");
                    body.push_str(&format!("smi_stolen_time_nanoseconds_total {}\n", tsc.stolen_time_ns_total));

                    body.push_str("# HELP smi_abduction_events_total Number of times CPU execution vanished exceeding latency threshold\n");
                    body.push_str("# TYPE smi_abduction_events_total counter\n");
                    body.push_str(&format!("smi_abduction_events_total {}\n", tsc.abduction_events_total));

                    body.push_str("# HELP smi_max_stolen_time_nanoseconds Longest single execution freeze caused by hardware firmware\n");
                    body.push_str("# TYPE smi_max_stolen_time_nanoseconds gauge\n");
                    body.push_str(&format!("smi_max_stolen_time_nanoseconds {}\n", tsc.max_stolen_time_ns));

                    body.push_str("# HELP smi_last_gap_nanoseconds Duration of the most recently detected stolen time gap\n");
                    body.push_str("# TYPE smi_last_gap_nanoseconds gauge\n");
                    body.push_str(&format!("smi_last_gap_nanoseconds {}\n", tsc.last_gap_ns));

                    body.push_str("# HELP smi_current_jitter_nanoseconds Peak RDTSC cycle delta jitter measured in sampling window\n");
                    body.push_str("# TYPE smi_current_jitter_nanoseconds gauge\n");
                    body.push_str(&format!("smi_current_jitter_nanoseconds {}\n", tsc.current_jitter_ns));

                    body.push_str("# HELP smi_tsc_frequency_hz Calibrated hardware Time Stamp Counter operating frequency in Hertz\n");
                    body.push_str("# TYPE smi_tsc_frequency_hz gauge\n");
                    body.push_str(&format!("smi_tsc_frequency_hz {}\n", tsc.tsc_hz));

                    body.push_str("# HELP smi_storm_active 1 if machine is actively suffering severe SMM time theft (>200us)\n");
                    body.push_str("# TYPE smi_storm_active gauge\n");
                    body.push_str(&format!("smi_storm_active {}\n", if tsc.is_storm_active { 1 } else { 0 }));

                    // Hardware interrupt counters
                    body.push_str("# HELP smi_interrupt_nmi_total Cumulative Non-Maskable Interrupts across all CPU cores\n");
                    body.push_str("# TYPE smi_interrupt_nmi_total counter\n");
                    body.push_str(&format!("smi_interrupt_nmi_total {}\n", irq.nmi_total));

                    body.push_str("# HELP smi_interrupt_tlb_shootdowns_total Cross-core MMU TLB cache invalidation shootdown IPIs\n");
                    body.push_str("# TYPE smi_interrupt_tlb_shootdowns_total counter\n");
                    body.push_str(&format!("smi_interrupt_tlb_shootdowns_total {}\n", irq.tlb_total));

                    body.push_str("# HELP smi_interrupt_reschedule_total Inter-processor rescheduling interrupts forcing thread context switches\n");
                    body.push_str("# TYPE smi_interrupt_reschedule_total counter\n");
                    body.push_str(&format!("smi_interrupt_reschedule_total {}\n", irq.res_total));

                    body.push_str("# HELP smi_interrupt_function_call_total Inter-processor cross-core function call synchronization interrupts\n");
                    body.push_str("# TYPE smi_interrupt_function_call_total counter\n");
                    body.push_str(&format!("smi_interrupt_function_call_total {}\n", irq.cal_total));

                    body.push_str("# HELP smi_interrupt_mce_total Hardware Machine Check Exceptions detected by CPU silicon\n");
                    body.push_str("# TYPE smi_interrupt_mce_total counter\n");
                    body.push_str(&format!("smi_interrupt_mce_total {}\n", irq.mce_total));

                    body.push_str("# HELP smi_interrupt_thermal_total Thermal threshold APIC event interrupts\n");
                    body.push_str("# TYPE smi_interrupt_thermal_total counter\n");
                    body.push_str(&format!("smi_interrupt_thermal_total {}\n", irq.trm_total));

                    let response = Response::from_string(body)
                        .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/plain; version=0.0.4"[..]).unwrap());
                    let _ = request.respond(response);
                } else {
                    let response = Response::from_string("Not Found").with_status_code(404);
                    let _ = request.respond(response);
                }
            }
        });
    }
}
