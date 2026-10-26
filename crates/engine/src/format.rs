//! Text formatting for values in notifications and reports (the UI has its own formatter
//! for live display; both follow the same rules).

use fpsframer_core::model::{NetUnit, Settings, TempUnit};
use fpsframer_core::sensors::Unit;

pub fn format_value(unit: Unit, v: f64, s: &Settings) -> String {
    match unit {
        Unit::Percent => format!("{v:.0}%"),
        Unit::Celsius => match s.units.temperature {
            TempUnit::Celsius => format!("{v:.0}°C"),
            TempUnit::Fahrenheit => format!("{:.0}°F", v * 9.0 / 5.0 + 32.0),
        },
        Unit::Mhz => {
            if v >= 1000.0 {
                format!("{:.2} GHz", v / 1000.0)
            } else {
                format!("{v:.0} MHz")
            }
        }
        Unit::Megabytes => {
            if v >= 1024.0 {
                format!("{:.1} GB", v / 1024.0)
            } else {
                format!("{v:.0} MB")
            }
        }
        Unit::BytesPerSec => match s.units.network {
            NetUnit::Megabits => format!("{:.1} Mbps", v * 8.0 / 1_000_000.0),
            NetUnit::Megabytes => format!("{:.1} MB/s", v / 1_000_000.0),
        },
        Unit::Watt => format!("{v:.0} W"),
        Unit::Volt => format!("{v:.2} V"),
        Unit::Rpm => format!("{v:.0} RPM"),
        Unit::Ms => format!("{v:.1} ms"),
        Unit::Fps => format!("{v:.0} FPS"),
        Unit::Seconds => fpsframer_core::analysis::fmt_clock(v),
        Unit::Count => format!("{v:.0}"),
        Unit::Text => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_follow_user_units() {
        let mut s = Settings::default();
        assert_eq!(format_value(Unit::Celsius, 90.0, &s), "90°C");
        s.units.temperature = TempUnit::Fahrenheit;
        assert_eq!(format_value(Unit::Celsius, 90.0, &s), "194°F");
        assert_eq!(format_value(Unit::BytesPerSec, 12_500_000.0, &s), "100.0 Mbps");
        s.units.network = NetUnit::Megabytes;
        assert_eq!(format_value(Unit::BytesPerSec, 12_500_000.0, &s), "12.5 MB/s");
        assert_eq!(format_value(Unit::Megabytes, 6144.0, &s), "6.0 GB");
        assert_eq!(format_value(Unit::Mhz, 4650.0, &s), "4.65 GHz");
        assert_eq!(format_value(Unit::Seconds, 3725.0, &s), "1:02:05");
    }
}
