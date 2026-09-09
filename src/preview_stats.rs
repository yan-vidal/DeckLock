//! Preview-only diagnostics. Process RSS/CPU are not attributed to one effect.
use gtk::prelude::*;
use std::time::Instant;
pub fn cpu_seconds(thread: bool) -> Option<f64> {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let clock = if thread {
        libc::CLOCK_THREAD_CPUTIME_ID
    } else {
        libc::CLOCK_PROCESS_CPUTIME_ID
    };
    // Valid pointer to a writable timespec; these clocks do not change system state.
    if unsafe { libc::clock_gettime(clock, &mut value) } != 0 {
        return None;
    }
    Some(value.tv_sec as f64 + value.tv_nsec as f64 / 1e9)
}
fn rss_kib(source: &str) -> Option<u64> {
    source.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")
            .and_then(|v| v.split_whitespace().next()?.parse().ok())
    })
}
fn percent(cpu: f64, wall: f64) -> f64 {
    if wall > 0. {
        cpu.max(0.) / wall * 100.
    } else {
        0.
    }
}
pub struct Metrics {
    label: glib::WeakRef<gtk::Label>,
    last: Instant,
    cpu: Option<f64>,
    frames: u64,
    render_cpu: f64,
    strings: std::rc::Rc<crate::i18n::I18n>,
}
impl Metrics {
    pub fn new(label: &gtk::Label, strings: std::rc::Rc<crate::i18n::I18n>) -> Self {
        label.set_text(&strings.text("procedural-stats-wait"));
        label.set_tooltip_text(Some(&strings.text("procedural-stats-help")));
        Self {
            label: label.downgrade(),
            last: Instant::now(),
            cpu: cpu_seconds(false),
            frames: 0,
            render_cpu: 0.,
            strings,
        }
    }
    pub fn record(&mut self, render_cpu: f64, bytes: usize) {
        self.frames += 1;
        self.render_cpu += render_cpu;
        let elapsed = self.last.elapsed().as_secs_f64();
        if elapsed < 1. {
            return;
        }
        let now = cpu_seconds(false);
        let cpu = self
            .cpu
            .zip(now)
            .map(|(before, after)| format!("{:.1}%", percent(after - before, elapsed)))
            .unwrap_or_else(|| "—".into());
        let memory = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| rss_kib(&s))
            .map(|k| format!("{:.1} MiB", k as f64 / 1024.))
            .unwrap_or_else(|| "—".into());
        if let Some(label) = self.label.upgrade() {
            label.set_tooltip_text(Some(&self.strings.text("procedural-stats-help")));
            label.set_text(&format!(
                "{} {:.1} · {} {:.1}% · {} {}\n{} {} · {:.0} KiB/texture",
                self.strings.text("procedural-fps"),
                self.frames as f64 / elapsed,
                self.strings.text("procedural-render-cpu"),
                percent(self.render_cpu, elapsed),
                self.strings.text("procedural-process-cpu"),
                cpu,
                self.strings.text("procedural-process-rss"),
                memory,
                bytes as f64 / 1024.
            ));
        }
        self.last = Instant::now();
        self.cpu = now;
        self.frames = 0;
        self.render_cpu = 0.;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counters_report_process_rss_and_one_core_cpu_units() {
        assert_eq!(
            rss_kib("Name: test\nVmSize: 200 kB\nVmRSS:\t123 kB\n"),
            Some(123)
        );
        assert_eq!(rss_kib("VmSize: 200 kB"), None);
        assert_eq!(percent(0.5, 2.), 25.);
        assert_eq!(percent(3., 2.), 150.);
        assert_eq!(percent(1., 0.), 0.);
    }
}
