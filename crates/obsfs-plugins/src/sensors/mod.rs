//! # Sensors Plugin - Hardware Sensors
//!
//! Provides information about temperature, fan speed, and voltage sensors from hardware.
//! Reads from both `/sys/class/hwmon` and `/sys/class/thermal` interfaces.

use std::fs;
use std::sync::Arc;

use anyhow::Result;
use obsfs_core::{MetricProvider, MetricValue, Plugin, Registry};

// =============================================================================
// SENSOR INFO
// =============================================================================

#[derive(Debug, Clone)]
struct SensorReading {
    device: String,
    label: String,
    value: f64,
    unit: String,
    critical: Option<f64>,
}

// =============================================================================
// SENSOR READER
// =============================================================================

struct SensorReader {
    hwmon_path: String,
    thermal_path: String,
}

impl SensorReader {
    fn new() -> Self {
        Self {
            hwmon_path: "/sys/class/hwmon".to_string(),
            thermal_path: "/sys/class/thermal".to_string(),
        }
    }

    fn read_file_value(&self, path: &str) -> Option<String> {
        fs::read_to_string(path).ok().map(|s| s.trim().to_string())
    }

    fn read_temperatures(&self) -> Vec<SensorReading> {
        let mut readings = Vec::new();

        // Read from hwmon
        if let Ok(entries) = fs::read_dir(&self.hwmon_path) {
            for entry in entries.flatten() {
                let hwmon_dir = entry.path();
                let device_name = self
                    .read_file_value(&hwmon_dir.join("name").to_string_lossy())
                    .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());

                // Look for temp*_input files
                if let Ok(files) = fs::read_dir(&hwmon_dir) {
                    for file in files.flatten() {
                        let filename = file.file_name().to_string_lossy().to_string();
                        if filename.starts_with("temp") && filename.ends_with("_input") {
                            let prefix = filename.trim_end_matches("_input");

                            // Read temperature (in millidegrees)
                            if let Some(value_str) =
                                self.read_file_value(&file.path().to_string_lossy())
                            {
                                if let Ok(millidegrees) = value_str.parse::<i64>() {
                                    let temp = millidegrees as f64 / 1000.0;

                                    // Try to get label
                                    let label_path = hwmon_dir.join(format!("{}_label", prefix));
                                    let label = self
                                        .read_file_value(&label_path.to_string_lossy())
                                        .unwrap_or_else(|| prefix.to_string());

                                    // Try to get critical temp
                                    let crit_path = hwmon_dir.join(format!("{}_crit", prefix));
                                    let critical = self
                                        .read_file_value(&crit_path.to_string_lossy())
                                        .and_then(|s| s.parse::<i64>().ok())
                                        .map(|v| v as f64 / 1000.0);

                                    readings.push(SensorReading {
                                        device: device_name.clone(),
                                        label,
                                        value: temp,
                                        unit: "°C".to_string(),
                                        critical,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Also read from thermal zones
        if let Ok(entries) = fs::read_dir(&self.thermal_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("thermal_zone") {
                    let zone_dir = entry.path();

                    let zone_type = self
                        .read_file_value(&zone_dir.join("type").to_string_lossy())
                        .unwrap_or_else(|| name.clone());

                    if let Some(value_str) =
                        self.read_file_value(&zone_dir.join("temp").to_string_lossy())
                    {
                        if let Ok(millidegrees) = value_str.parse::<i64>() {
                            let temp = millidegrees as f64 / 1000.0;

                            readings.push(SensorReading {
                                device: "thermal".to_string(),
                                label: zone_type,
                                value: temp,
                                unit: "°C".to_string(),
                                critical: None,
                            });
                        }
                    }
                }
            }
        }

        readings
    }

    fn read_fans(&self) -> Vec<SensorReading> {
        let mut readings = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.hwmon_path) {
            for entry in entries.flatten() {
                let hwmon_dir = entry.path();
                let device_name = self
                    .read_file_value(&hwmon_dir.join("name").to_string_lossy())
                    .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());

                // Look for fan*_input files
                if let Ok(files) = fs::read_dir(&hwmon_dir) {
                    for file in files.flatten() {
                        let filename = file.file_name().to_string_lossy().to_string();
                        if filename.starts_with("fan") && filename.ends_with("_input") {
                            let prefix = filename.trim_end_matches("_input");

                            if let Some(value_str) =
                                self.read_file_value(&file.path().to_string_lossy())
                            {
                                if let Ok(rpm) = value_str.parse::<i64>() {
                                    let label_path = hwmon_dir.join(format!("{}_label", prefix));
                                    let label = self
                                        .read_file_value(&label_path.to_string_lossy())
                                        .unwrap_or_else(|| prefix.to_string());

                                    readings.push(SensorReading {
                                        device: device_name.clone(),
                                        label,
                                        value: rpm as f64,
                                        unit: "RPM".to_string(),
                                        critical: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        readings
    }
}

// =============================================================================
// PROVIDERS
// =============================================================================

/// Provider for temperature sensors.
pub struct TemperaturesProvider;

impl TemperaturesProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TemperaturesProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for TemperaturesProvider {
    fn path(&self) -> &str {
        "sensors/temperatures"
    }

    fn collect(&self) -> Result<MetricValue> {
        let reader = SensorReader::new();
        let temps = reader.read_temperatures();

        let mut out = String::new();
        out.push_str("Temperature Sensors\n");
        out.push_str(&"=".repeat(50));
        out.push_str("\n\n");

        if temps.is_empty() {
            out.push_str("No temperature sensors found\n");
        } else {
            for temp in &temps {
                let warning = temp
                    .critical
                    .map(|c| {
                        if temp.value >= c {
                            " [CRITICAL]"
                        } else if temp.value >= c * 0.9 {
                            " [WARN]"
                        } else {
                            ""
                        }
                    })
                    .unwrap_or("");

                out.push_str(&format!(
                    "{}/{}: {:.1}{}{}\n",
                    temp.device, temp.label, temp.value, temp.unit, warning
                ));
            }
        }

        Ok(MetricValue::Text(out))
    }
}

/// Provider for fan sensors.
pub struct FansProvider;

impl FansProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FansProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for FansProvider {
    fn path(&self) -> &str {
        "sensors/fans"
    }

    fn collect(&self) -> Result<MetricValue> {
        let reader = SensorReader::new();
        let fans = reader.read_fans();

        let mut out = String::new();
        out.push_str("Fan Sensors\n");
        out.push_str(&"=".repeat(50));
        out.push_str("\n\n");

        if fans.is_empty() {
            out.push_str("No fan sensors found\n");
        } else {
            for fan in &fans {
                let status = if fan.value == 0.0 { " [STOPPED]" } else { "" };
                out.push_str(&format!(
                    "{}/{}: {:.0} {}{}\n",
                    fan.device, fan.label, fan.value, fan.unit, status
                ));
            }
        }

        Ok(MetricValue::Text(out))
    }
}

/// Provider for sensor summary.
pub struct SensorSummaryProvider;

impl SensorSummaryProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SensorSummaryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for SensorSummaryProvider {
    fn path(&self) -> &str {
        "sensors/summary"
    }

    fn collect(&self) -> Result<MetricValue> {
        let reader = SensorReader::new();
        let temps = reader.read_temperatures();
        let fans = reader.read_fans();

        let mut out = String::new();
        out.push_str("Hardware Sensors\n");
        out.push_str(&"=".repeat(50));
        out.push_str("\n\n");

        // Temperatures
        out.push_str("Temperatures:\n");
        if temps.is_empty() {
            out.push_str("  No sensors found\n");
        } else {
            let mut has_warning = false;
            let mut has_critical = false;

            for temp in &temps {
                let (warning, status) = temp
                    .critical
                    .map(|c| {
                        if temp.value >= c {
                            has_critical = true;
                            (true, " [CRITICAL]")
                        } else if temp.value >= c * 0.9 {
                            has_warning = true;
                            (true, " [WARN]")
                        } else {
                            (false, "")
                        }
                    })
                    .unwrap_or((false, ""));

                let _ = warning;
                out.push_str(&format!(
                    "  {}/{}: {:.1}{}{}\n",
                    temp.device, temp.label, temp.value, temp.unit, status
                ));
            }

            out.push('\n');

            // Fans
            out.push_str("Fans:\n");
            if fans.is_empty() {
                out.push_str("  No sensors found\n");
            } else {
                for fan in &fans {
                    let status = if fan.value == 0.0 { " [STOPPED]" } else { "" };
                    out.push_str(&format!(
                        "  {}/{}: {:.0} {}{}\n",
                        fan.device, fan.label, fan.value, fan.unit, status
                    ));
                }
            }

            out.push('\n');

            // Overall status
            let status = if has_critical {
                "CRITICAL (temperature above threshold)"
            } else if has_warning {
                "WARNING (temperature approaching threshold)"
            } else {
                "OK (all temperatures normal)"
            };
            out.push_str(&format!("Status: {}\n", status));
        }

        Ok(MetricValue::Text(out))
    }
}

// =============================================================================
// SENSORS PLUGIN
// =============================================================================

/// Plugin that provides hardware sensor information.
pub struct SensorsPlugin;

impl SensorsPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for SensorsPlugin {
    fn name(&self) -> &str {
        "sensors"
    }

    fn description(&self) -> &str {
        "Hardware sensors (temperature, fans) at /obs/sensors/"
    }

    fn register(&self, registry: &mut Registry) -> Result<()> {
        registry
            .insert_provider(Arc::new(TemperaturesProvider::new()))
            .map_err(|e| anyhow::anyhow!(e))?;

        registry
            .insert_provider(Arc::new(FansProvider::new()))
            .map_err(|e| anyhow::anyhow!(e))?;

        registry
            .insert_provider(Arc::new(SensorSummaryProvider::new()))
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(())
    }
}

impl Default for SensorsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_metadata() {
        let plugin = SensorsPlugin::new();
        assert_eq!(plugin.name(), "sensors");
        assert!(!plugin.description().is_empty());
    }

    #[test]
    fn test_sensor_reader() {
        let reader = SensorReader::new();
        // Just test that it doesn't panic
        let _ = reader.read_temperatures();
        let _ = reader.read_fans();
    }
}
