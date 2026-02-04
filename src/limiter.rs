use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;
use sysinfo::System;
use parking_lot::Mutex;

#[derive(Clone, Debug)]
pub struct LimiterState {
    pub target_pid: Option<i32>,
    pub limit_percentage: u32, // 1-100
    pub mode: LimiterMode,
    pub is_active: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LimiterMode {
    Targeted, // Limit specific PID
    Global,   // Keep total system CPU below limit
}

pub struct Limiter {
    state: Arc<Mutex<LimiterState>>,
    stop_signal: Arc<AtomicBool>,
}

impl Limiter {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(LimiterState {
                target_pid: None,
                limit_percentage: 100, // No limit by default
                mode: LimiterMode::Targeted,
                is_active: false,
            })),
            stop_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_target(&self, pid: i32) {
        let mut state = self.state.lock();
        state.target_pid = Some(pid);
        state.mode = LimiterMode::Targeted;
        // Resume previous target if changed? 
        // For simplicity, the worker handles cleanup when state changes or on loop.
    }

    pub fn set_global(&self, limit: u32) {
        let mut state = self.state.lock();
        state.mode = LimiterMode::Global;
        state.limit_percentage = limit;
    }

    pub fn set_limit(&self, limit: u32) {
        let mut state = self.state.lock();
        state.limit_percentage = limit.clamp(1, 100);
    }

    pub fn toggle(&self, active: bool) {
        let mut state = self.state.lock();
        state.is_active = active;
        if !active {
            // Ensure we resume the process if we stop limiting
            if let Some(pid) = state.target_pid {
                let _ = kill(Pid::from_raw(pid), Signal::SIGCONT);
            }
        }
    }

    #[allow(dead_code)]
    pub fn get_state(&self) -> LimiterState {
        self.state.lock().clone()
    }

    pub fn start_background_task(&self) {
        let state_handle = self.state.clone();
        let stop_handle = self.stop_signal.clone();

        thread::spawn(move || {
            let mut sys = System::new_all();
            let mut currently_throttled_pid: Option<i32> = None;

            // Duty cycle period in ms
            const PERIOD_MS: u64 = 100;

            loop {
                if stop_handle.load(Ordering::Relaxed) {
                    break;
                }

                let (target, limit, mode, active) = {
                    let s = state_handle.lock();
                    (s.target_pid, s.limit_percentage, s.mode, s.is_active)
                };

                if !active {
                    // Make sure we release any throttled process
                    if let Some(pid) = currently_throttled_pid {
                         let _ = kill(Pid::from_raw(pid), Signal::SIGCONT);
                         currently_throttled_pid = None;
                    }
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }

                let pid_to_throttle = match mode {
                    LimiterMode::Targeted => target,
                    LimiterMode::Global => {
                        // Global logic:
                        // Refresh CPU usage
                        sys.refresh_cpu_all();
                        let total_load = sys.global_cpu_usage();
                        
                        // If total load > limit, throttle the highest consumer that isn't us
                        if total_load > limit as f32 {
                            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
                            // Find highest cpu process, excluding ourselves
                            let myself = std::process::id() as i32;
                            sys.processes().iter()
                                .filter(|(pid, _)| pid.as_u32() as i32 != myself)
                                .max_by(|(_, a), (_, b)| a.cpu_usage().partial_cmp(&b.cpu_usage()).unwrap_or(std::cmp::Ordering::Equal))
                                .map(|(pid, _)| pid.as_u32() as i32)
                        } else {
                            None
                        }
                    }
                };

                // If target changed, resume old one
                if currently_throttled_pid != pid_to_throttle {
                     if let Some(old_pid) = currently_throttled_pid {
                         // Resume old
                         let _ = kill(Pid::from_raw(old_pid), Signal::SIGCONT);
                     }
                     currently_throttled_pid = pid_to_throttle;
                }

                if let Some(pid) = pid_to_throttle {
                    // Duty cycle logic
                    // If limit is 50%, run 50ms, stop 50ms.
                    // If limit is 20%, run 20ms, stop 80ms.
                    let run_ms = (PERIOD_MS * limit as u64) / 100;
                    let stop_ms = PERIOD_MS - run_ms;

                    // To be safe, verify process exists before sending
                    // Nix kill essentially checks existence (ESRCH)
                    
                    // Resume (Run)
                    if let Err(_) = kill(Pid::from_raw(pid), Signal::SIGCONT) {
                        // Process dead?
                        currently_throttled_pid = None;
                        continue;
                    }
                    thread::sleep(Duration::from_millis(run_ms));

                    // Stop
                    if stop_ms > 0 {
                         if let Err(_) = kill(Pid::from_raw(pid), Signal::SIGSTOP) {
                             currently_throttled_pid = None;
                             continue;
                         }
                         thread::sleep(Duration::from_millis(stop_ms));
                    }
                } else {
                    thread::sleep(Duration::from_millis(PERIOD_MS));
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limiter_state_changes() {
        let limiter = Limiter::new();
        
        // Initial state
        let state = limiter.get_state();
        assert_eq!(state.mode, LimiterMode::Targeted);
        assert_eq!(state.is_active, false);

        // Set target
        limiter.set_target(1234);
        let state = limiter.get_state();
        assert_eq!(state.target_pid, Some(1234));
        assert_eq!(state.mode, LimiterMode::Targeted);

        // Toggle active
        limiter.toggle(true);
        let state = limiter.get_state();
        assert_eq!(state.is_active, true);

        // Set global
        limiter.set_global(80);
        let state = limiter.get_state();
        assert_eq!(state.mode, LimiterMode::Global);
        assert_eq!(state.limit_percentage, 80);

        // Back to target
        limiter.set_target(5678);
        let state = limiter.get_state();
        assert_eq!(state.mode, LimiterMode::Targeted);
        assert_eq!(state.target_pid, Some(5678));
    }
}
