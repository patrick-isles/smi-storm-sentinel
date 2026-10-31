//! High-Frequency x86_64 RDTSC Hardware Latency & Stolen Time Sampler
//!
//! Executes the `RDTSC` assembly instruction directly against silicon to detect time gaps
//! when the CPU is abducted into Ring -2 System Management Mode (SMM) by BIOS firmware.

use std::arch::x86_64::_rdtsc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TscStats {
    pub tsc_hz: u64,
    pub threshold_ns: u64,
    pub stolen_time_ns_total: u64,
    pub abduction_events_total: u64,
    pub max_stolen_time_ns: u64,
    pub last_gap_ns: u64,
    pub current_jitter_ns: u64,
    pub is_storm_active: bool,
}

pub struct TscSampler {
    tsc_hz: u64,
    threshold_ns: u64,
    stolen_time_ns_total: Arc<AtomicU64>,
    abduction_events_total: Arc<AtomicU64>,
    max_stolen_time_ns: Arc<AtomicU64>,
    last_gap_ns: Arc<AtomicU64>,
    current_jitter_ns: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
}

impl TscSampler {
    /// Calibrates the CPU Time Stamp Counter (TSC) frequency and initializes the sampler.
    pub fn new(threshold_ns: u64) -> Self {
        let tsc_hz = Self::calibrate_tsc_hz();

        Self {
            tsc_hz,
            threshold_ns,
            stolen_time_ns_total: Arc::new(AtomicU64::new(0)),
            abduction_events_total: Arc::new(AtomicU64::new(0)),
            max_stolen_time_ns: Arc::new(AtomicU64::new(0)),
            last_gap_ns: Arc::new(AtomicU64::new(0)),
            current_jitter_ns: Arc::new(AtomicU64::new(0)),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Spawns the dedicated HWLAT hardware latency detection thread.
    pub fn start_sampling(&self) {
        let tsc_hz = self.tsc_hz;
        let threshold_cycles = (self.threshold_ns as f64 * (tsc_hz as f64 / 1_000_000_000.0)) as u64;

        let stolen_total = self.stolen_time_ns_total.clone();
        let events_total = self.abduction_events_total.clone();
        let max_stolen = self.max_stolen_time_ns.clone();
        let last_gap = self.last_gap_ns.clone();
        let current_jitter = self.current_jitter_ns.clone();
        let running = self.running.clone();

        thread::spawn(move || {
            let cycles_to_ns = 1_000_000_000.0 / (tsc_hz as f64);

            while running.load(Ordering::Relaxed) {
                // Run tight sampling burst of 50,000 read iterations
                let mut prev_tsc = unsafe { _rdtsc() };
                let mut burst_max_delta = 0u64;

                for _ in 0..50_000 {
                    let now_tsc = unsafe { _rdtsc() };
                    let delta_cycles = now_tsc.saturating_sub(prev_tsc);

                    if delta_cycles > burst_max_delta {
                        burst_max_delta = delta_cycles;
                    }

                    // Did the CPU vanish longer than our threshold?
                    if delta_cycles >= threshold_cycles {
                        let gap_ns = (delta_cycles as f64 * cycles_to_ns) as u64;

                        stolen_total.fetch_add(gap_ns, Ordering::Relaxed);
                        events_total.fetch_add(1, Ordering::Relaxed);
                        last_gap.store(gap_ns, Ordering::Relaxed);

                        // Update max if greater
                        let mut curr_max = max_stolen.load(Ordering::Relaxed);
                        while gap_ns > curr_max {
                            match max_stolen.compare_exchange_weak(
                                curr_max,
                                gap_ns,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            ) {
                                Ok(_) => break,
                                Err(actual) => curr_max = actual,
                            }
                        }
                    }

                    prev_tsc = now_tsc;
                }

                let burst_jitter_ns = (burst_max_delta as f64 * cycles_to_ns) as u64;
                current_jitter.store(burst_jitter_ns, Ordering::Relaxed);

                // Sleep 2ms between bursts to avoid starving other CPU cores
                thread::sleep(Duration::from_millis(2));
            }
        });
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn snapshot(&self) -> TscStats {
        let max_gap = self.max_stolen_time_ns.load(Ordering::Relaxed);
        let last = self.last_gap_ns.load(Ordering::Relaxed);
        let jitter = self.current_jitter_ns.load(Ordering::Relaxed);

        // An SMI storm is considered active if we experienced an abduction gap > 200us recently
        let is_storm_active = last >= 200_000 || jitter >= 200_000;

        TscStats {
            tsc_hz: self.tsc_hz,
            threshold_ns: self.threshold_ns,
            stolen_time_ns_total: self.stolen_time_ns_total.load(Ordering::Relaxed),
            abduction_events_total: self.abduction_events_total.load(Ordering::Relaxed),
            max_stolen_time_ns: max_gap,
            last_gap_ns: last,
            current_jitter_ns: jitter,
            is_storm_active,
        }
    }

    fn calibrate_tsc_hz() -> u64 {
        let start = Instant::now();
        let t0 = unsafe { _rdtsc() };
        thread::sleep(Duration::from_millis(50));
        let t1 = unsafe { _rdtsc() };
        let elapsed = start.elapsed().as_secs_f64();
        let cycles = t1.saturating_sub(t0);

        if elapsed > 0.0 {
            (cycles as f64 / elapsed) as u64
        } else {
            3_000_000_000
        }
    }
}
