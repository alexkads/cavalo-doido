use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use parking_lot::Mutex;
use std::collections::{HashSet, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;
use sysinfo::System;

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
            let mut targeted_pid: Option<i32> = None;
            let mut paused_global: VecDeque<i32> = VecDeque::new();
            let mut paused_global_set: HashSet<i32> = HashSet::new();

            // Duty cycle period in ms
            const PERIOD_MS: u64 = 100;
            const GLOBAL_HYSTERESIS: f32 = 5.0;

            loop {
                if stop_handle.load(Ordering::Relaxed) {
                    if let Some(pid) = targeted_pid.take() {
                        let _ = kill(Pid::from_raw(pid), Signal::SIGCONT);
                    }
                    while let Some(pid) = paused_global.pop_front() {
                        let _ = kill(Pid::from_raw(pid), Signal::SIGCONT);
                        paused_global_set.remove(&pid);
                    }
                    break;
                }

                let (target, limit, mode, active) = {
                    let s = state_handle.lock();
                    (s.target_pid, s.limit_percentage, s.mode, s.is_active)
                };

                if !active {
                    if let Some(pid) = targeted_pid.take() {
                        let _ = kill(Pid::from_raw(pid), Signal::SIGCONT);
                    }
                    while let Some(pid) = paused_global.pop_front() {
                        let _ = kill(Pid::from_raw(pid), Signal::SIGCONT);
                        paused_global_set.remove(&pid);
                    }
                    thread::sleep(Duration::from_millis(PERIOD_MS));
                    continue;
                }

                match mode {
                    LimiterMode::Targeted => {
                        if !paused_global.is_empty() {
                            while let Some(pid) = paused_global.pop_front() {
                                let _ = kill(Pid::from_raw(pid), Signal::SIGCONT);
                                paused_global_set.remove(&pid);
                            }
                        }

                        if targeted_pid != target {
                            if let Some(pid) = targeted_pid.take() {
                                let _ = kill(Pid::from_raw(pid), Signal::SIGCONT);
                            }
                            targeted_pid = target;
                        }

                        if let Some(pid) = targeted_pid {
                            let run_ms = (PERIOD_MS * limit as u64) / 100;
                            let stop_ms = PERIOD_MS - run_ms;

                            if kill(Pid::from_raw(pid), Signal::SIGCONT).is_err() {
                                targeted_pid = None;
                                thread::sleep(Duration::from_millis(PERIOD_MS));
                                continue;
                            }
                            if run_ms > 0 {
                                thread::sleep(Duration::from_millis(run_ms));
                            }

                            if stop_ms > 0 {
                                if kill(Pid::from_raw(pid), Signal::SIGSTOP).is_err() {
                                    targeted_pid = None;
                                    thread::sleep(Duration::from_millis(PERIOD_MS));
                                    continue;
                                }
                                thread::sleep(Duration::from_millis(stop_ms));
                            }
                        } else {
                            thread::sleep(Duration::from_millis(PERIOD_MS));
                        }
                    }
                    LimiterMode::Global => {
                        if let Some(pid) = targeted_pid.take() {
                            let _ = kill(Pid::from_raw(pid), Signal::SIGCONT);
                        }

                        sys.refresh_cpu_all();
                        let total_load = sys.global_cpu_usage();
                        let limit_f32 = limit as f32;
                        let lower_threshold = (limit_f32 - GLOBAL_HYSTERESIS).max(0.0);

                        if total_load > limit_f32 {
                            sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
                            let myself = std::process::id() as i32;

                            let mut candidates: Vec<_> = sys
                                .processes()
                                .iter()
                                .filter_map(|(pid, process)| {
                                    let pid_i32 = pid.as_u32() as i32;
                                    if pid_i32 == myself || paused_global_set.contains(&pid_i32) {
                                        return None;
                                    }
                                    let usage = process.cpu_usage();
                                    if usage <= 0.5 {
                                        return None;
                                    }
                                    Some((pid_i32, usage))
                                })
                                .collect();

                            candidates.sort_by(|a, b| {
                                b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                            });

                            if let Some((pid, _)) = candidates.first() {
                                let pid_i32 = *pid;
                                if kill(Pid::from_raw(pid_i32), Signal::SIGSTOP).is_ok() {
                                    paused_global_set.insert(pid_i32);
                                    paused_global.push_back(pid_i32);
                                } else {
                                    paused_global_set.remove(&pid_i32);
                                }
                            }
                        } else if total_load < lower_threshold {
                            if let Some(pid) = paused_global.pop_front() {
                                let _ = kill(Pid::from_raw(pid), Signal::SIGCONT);
                                paused_global_set.remove(&pid);
                            }
                        }

                        thread::sleep(Duration::from_millis(PERIOD_MS));
                    }
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
