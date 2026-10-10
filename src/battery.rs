//! The system battery from the Linux power-supply class, for display only.
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    pub percent: u8,
    /// On external power: charging, or full and plugged in.
    pub charging: bool,
}

fn field(supply: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(supply.join(name))
        .ok()
        .map(|value| value.trim().to_owned())
}

/// The system batteries under `root`, averaged. Mains adapters, absent cells and
/// batteries of peripherals such as a controller (scope Device) are ignored.
pub fn read(root: &Path) -> Option<Level> {
    let mut total = 0_u32;
    let mut count = 0_u32;
    let mut charging = false;
    for supply in std::fs::read_dir(root).ok()?.flatten().map(|e| e.path()) {
        if field(&supply, "type").as_deref() != Some("Battery")
            || field(&supply, "scope").as_deref() == Some("Device")
            || field(&supply, "present").as_deref() == Some("0")
        {
            continue;
        }
        let Some(capacity) = field(&supply, "capacity").and_then(|c| c.parse::<u32>().ok()) else {
            continue;
        };
        total += capacity.min(100);
        count += 1;
        charging |= matches!(
            field(&supply, "status").as_deref(),
            Some("Charging" | "Full")
        );
    }
    (count > 0).then(|| Level {
        percent: ((total + count / 2) / count) as u8,
        charging,
    })
}

/// Adwaita's battery status icon for the level, at the nearest ten.
pub fn icon_name(level: Level) -> String {
    let tens = ((u32::from(level.percent) + 5) / 10 * 10).min(100);
    match (tens, level.charging) {
        (100, true) => "battery-level-100-charged-symbolic".into(),
        (tens, true) => format!("battery-level-{tens}-charging-symbolic"),
        (tens, false) => format!("battery-level-{tens}-symbolic"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn supply(root: &Path, name: &str, fields: &[(&str, &str)]) {
        let path = root.join(name);
        fs::create_dir(&path).unwrap();
        for (field, value) in fields {
            fs::write(path.join(field), format!("{value}\n")).unwrap();
        }
    }

    #[test]
    fn reads_the_system_battery_and_ignores_mains_and_device_batteries() {
        let root = tempfile::tempdir().unwrap();
        supply(root.path(), "ACAD", &[("type", "Mains"), ("online", "1")]);
        supply(
            root.path(),
            "BAT1",
            &[
                ("type", "Battery"),
                ("present", "1"),
                ("capacity", "67"),
                ("status", "Charging"),
            ],
        );
        supply(
            root.path(),
            "controller",
            &[
                ("type", "Battery"),
                ("scope", "Device"),
                ("capacity", "5"),
                ("status", "Discharging"),
            ],
        );
        assert_eq!(
            read(root.path()),
            Some(Level {
                percent: 67,
                charging: true
            })
        );
    }

    #[test]
    fn no_battery_absent_cell_or_unreadable_root_reports_nothing() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(read(root.path()), None);
        supply(root.path(), "ADP1", &[("type", "Mains"), ("online", "1")]);
        supply(
            root.path(),
            "BAT0",
            &[("type", "Battery"), ("present", "0"), ("capacity", "0")],
        );
        assert_eq!(read(root.path()), None);
        assert_eq!(read(&root.path().join("missing")), None);
    }

    #[test]
    fn several_system_batteries_are_averaged_and_capacity_is_bounded() {
        let root = tempfile::tempdir().unwrap();
        supply(
            root.path(),
            "BAT0",
            &[
                ("type", "Battery"),
                ("capacity", "40"),
                ("status", "Discharging"),
            ],
        );
        supply(
            root.path(),
            "BAT1",
            &[("type", "Battery"), ("capacity", "250"), ("status", "Full")],
        );
        assert_eq!(
            read(root.path()),
            Some(Level {
                percent: 70,
                charging: true
            })
        );
    }

    #[test]
    fn icon_follows_the_nearest_ten_and_power_state() {
        let level = |percent, charging| Level { percent, charging };
        assert_eq!(icon_name(level(67, false)), "battery-level-70-symbolic");
        assert_eq!(icon_name(level(4, false)), "battery-level-0-symbolic");
        assert_eq!(
            icon_name(level(67, true)),
            "battery-level-70-charging-symbolic"
        );
        assert_eq!(icon_name(level(100, false)), "battery-level-100-symbolic");
        assert_eq!(
            icon_name(level(98, true)),
            "battery-level-100-charged-symbolic"
        );
    }
}
