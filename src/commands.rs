//! Local commands and session-only settings; every feature starts disabled.
use std::sync::Mutex;

pub const USAGES: [&str; 5] = [
    "cfly toggle",
    "cfly elytratoggle",
    "cfly nofalltoggle",
    "cfly forceflighttoggle",
    "cfly speed <value:number>",
];

static SETTINGS: Mutex<Settings> = Mutex::new(Settings::new());

struct Settings {
    flight: bool,
    force_flight: bool,
    elytra: bool,
    speed: f64,
}

impl Settings {
    const fn new() -> Self {
        Self {
            flight: false,
            force_flight: false,
            elytra: false,
            speed: 1.0,
        }
    }

    fn flight_enabled(&self) -> bool {
        self.flight || self.force_flight
    }

    fn execute(&mut self, args: &[&str]) -> String {
        match args {
            ["toggle"] if self.force_flight => {
                "Flight: ON; force flight is ON. Use /cfly forceflighttoggle first.".into()
            }
            ["toggle"] => {
                self.flight = !self.flight;
                format!("Flight: {}. Double-tap jump to start flying.", status(self.flight))
            }
            ["forceflighttoggle"] => {
                self.force_flight = !self.force_flight;
                // Like Java, turning force mode off does not revoke existing permission.
                if self.force_flight {
                    self.flight = true;
                }
                format!(
                    "Force flight: {}; flight: {}.",
                    status(self.force_flight),
                    status(self.flight_enabled())
                )
            }
            ["elytratoggle"] => {
                self.elytra = !self.elytra;
                format!(
                    "Elytra setting: {} (placeholder; elytra boost is not implemented).",
                    status(self.elytra)
                )
            }
            ["speed", value] => match value.parse::<f64>() {
                Ok(speed) if speed.is_finite() && speed >= 0.0 => {
                    self.speed = speed;
                    format!("Elytra speed: {speed} (stored only; elytra boost is not implemented).")
                }
                _ => "Speed must be a finite number >= 0; setting unchanged.".into(),
            },
            _ => "Usage: /cfly toggle | elytratoggle | nofalltoggle | forceflighttoggle | speed <number >= 0>".into(),
        }
    }
}

fn status(enabled: bool) -> &'static str {
    if enabled { "ON" } else { "OFF" }
}

pub fn flight_enabled() -> bool {
    SETTINGS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .flight_enabled()
}

/// Consume the whole cfly namespace, including invalid usages, before server dispatch.
pub fn run(command: &str) -> Option<String> {
    let mut words = command.split_whitespace();
    if words.next()? != "cfly" {
        return None;
    }
    let args: Vec<_> = words.collect();
    let mut settings = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    let reply = if args == ["nofalltoggle"] {
        let enabled = !crate::nofall::is_enabled();
        crate::nofall::set_enabled(enabled);
        format!("No-fall: {}.", status(enabled))
    } else {
        settings.execute(&args)
    };
    if !settings.flight_enabled() {
        crate::correction::clear();
    }
    Some(reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_off_and_toggle_controls_flight() {
        let mut settings = Settings::new();
        assert!(!settings.flight_enabled());
        assert!(!settings.force_flight);
        assert!(!settings.elytra);
        assert_eq!(settings.speed, 1.0);
        assert!(settings.execute(&["toggle"]).contains("ON"));
        assert!(settings.flight_enabled());
        assert!(settings.execute(&["toggle"]).contains("OFF"));
        assert!(!settings.flight_enabled());
    }

    #[test]
    fn force_mode_blocks_toggle_until_disabled() {
        let mut settings = Settings::new();
        settings.execute(&["forceflighttoggle"]);
        assert!(settings.flight_enabled());
        assert!(settings.execute(&["toggle"]).contains("force flight is ON"));
        assert!(settings.flight_enabled());
        settings.execute(&["forceflighttoggle"]);
        assert!(!settings.force_flight);
        assert!(settings.flight_enabled());
        settings.execute(&["toggle"]);
        assert!(!settings.flight_enabled());
    }

    #[test]
    fn elytra_is_only_a_placeholder_and_speed_is_validated() {
        let mut settings = Settings::new();
        assert!(
            settings
                .execute(&["elytratoggle"])
                .contains("not implemented")
        );
        assert!(settings.elytra);
        settings.execute(&["elytratoggle"]);
        assert!(!settings.elytra);
        for value in ["0", "2.5", "1e3"] {
            settings.execute(&["speed", value]);
            assert_eq!(settings.speed, value.parse::<f64>().unwrap());
        }
        for value in ["-1", "NaN", "inf", "-inf", "1e999", "oops"] {
            assert!(settings.execute(&["speed", value]).contains("unchanged"));
            assert_eq!(settings.speed, 1000.0);
        }
        for args in [
            vec![],
            vec!["unknown"],
            vec!["toggle", "extra"],
            vec!["speed"],
        ] {
            assert!(settings.execute(&args).starts_with("Usage:"));
        }
        assert!(!settings.flight_enabled());
    }

    #[test]
    fn other_commands_are_not_consumed() {
        assert!(run("help").is_none());
        assert!(run("cflyother toggle").is_none());
        assert!(run("").is_none());
    }
}
