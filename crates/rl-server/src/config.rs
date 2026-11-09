//! The server launch settings, in the same JSON shape V2 used (`.rustyserver.json`), so servers
//! configured in V2 keep their settings.

use serde::{Deserialize, Serialize};

/// A setting with an on/off switch and a text value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextOption {
    pub enabled: bool,
    pub value: String,
}

impl Default for TextOption {
    fn default() -> Self {
        Self {
            enabled: true,
            value: String::new(),
        }
    }
}

impl TextOption {
    fn on(value: &str) -> Self {
        Self {
            enabled: true,
            value: value.to_string(),
        }
    }
    fn off(value: &str) -> Self {
        Self {
            enabled: false,
            value: value.to_string(),
        }
    }
}

/// A switch with no value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlagOption {
    pub enabled: bool,
}

impl Default for FlagOption {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerLaunchConfig {
    pub hostname: TextOption,
    /// The save-folder name (`server/<identity>`).
    pub identity: TextOption,
    pub port: TextOption,
    pub rcon_port: TextOption,
    pub rcon_password: TextOption,
    /// Use the WebSocket RCON (`rcon.web 1`), which the launcher's console tab needs.
    pub rcon_web: FlagOption,
    pub max_players: TextOption,
    pub world_size: TextOption,
    pub seed: TextOption,
    pub save_interval: TextOption,
    /// Extra arguments, passed through as written.
    pub custom_args: String,
}

impl Default for ServerLaunchConfig {
    /// The V2 defaults (including its `changeme` password). New servers use [`ServerLaunchConfig::for_new_server`].
    fn default() -> Self {
        Self {
            hostname: TextOption::on("My Rust Server"),
            identity: TextOption::on("rustyserver"),
            port: TextOption::on("28015"),
            rcon_port: TextOption::on("28016"),
            rcon_password: TextOption::on("changeme"),
            rcon_web: FlagOption { enabled: true },
            max_players: TextOption::on("50"),
            world_size: TextOption::on("3000"),
            seed: TextOption::off("12345"),
            save_interval: TextOption::on("300"),
            custom_args: String::new(),
        }
    }
}

/// Characters for generated passwords: no look-alikes (0/O, 1/l/I).
const PASSWORD_ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// A random RCON password. Anyone who knows it can run commands on the server, so a fixed
/// default like `changeme` is never used for a new server.
pub fn random_password(len: usize) -> String {
    let mut out = String::with_capacity(len);
    let mut buf = [0u8; 1];
    // Rejection sampling keeps every character equally likely.
    let limit = 256 - (256 % PASSWORD_ALPHABET.len());
    while out.len() < len {
        if getrandom::fill(&mut buf).is_err() {
            // The system random source failed; fall back to the clock rather than a fixed password.
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            buf[0] = (t ^ (t >> 8) ^ (out.len() as u128 * 31)) as u8;
        }
        if (buf[0] as usize) < limit {
            out.push(PASSWORD_ALPHABET[buf[0] as usize % PASSWORD_ALPHABET.len()] as char);
        }
    }
    out
}

/// Something wrong with a setting, in words the user can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigProblem {
    pub field: &'static str,
    pub message: String,
}

impl ServerLaunchConfig {
    /// Defaults for a freshly installed server: V2's, but with a random RCON password, and the
    /// server name and save-folder name taken from the name the user gave it.
    pub fn for_new_server(name: &str) -> Self {
        let mut c = Self::default();
        c.rcon_password.value = random_password(16);
        let identity = sanitize_identity(name);
        if !identity.is_empty() {
            c.identity.value = identity;
        }
        if !name.trim().is_empty() && !name.contains(['"', '\r', '\n']) {
            c.hostname.value = name.trim().to_string();
        }
        c
    }

    /// The command-line arguments. With `quote`, values that may contain spaces are wrapped in double
    /// quotes (for the `.bat` file). Without it they are plain, ready to hand to a process directly.
    pub fn args(&self, quote: bool) -> Vec<String> {
        let q = |v: &str| {
            if quote {
                format!("\"{v}\"")
            } else {
                v.to_string()
            }
        };
        let mut a: Vec<String> = vec!["-batchmode".into(), "+server.secure".into(), "1".into()];
        let text = |a: &mut Vec<String>, flag: &str, opt: &TextOption, quoted: bool| {
            if opt.enabled && !opt.value.is_empty() {
                a.push(flag.to_string());
                a.push(if quoted {
                    q(&opt.value)
                } else {
                    opt.value.clone()
                });
            }
        };
        text(&mut a, "+server.hostname", &self.hostname, true);
        text(&mut a, "+server.identity", &self.identity, true);
        text(&mut a, "+server.port", &self.port, false);
        text(&mut a, "+rcon.port", &self.rcon_port, false);
        text(&mut a, "+rcon.password", &self.rcon_password, true);
        if self.rcon_web.enabled {
            a.push("+rcon.web".into());
            a.push("1".into());
        }
        text(&mut a, "+server.maxplayers", &self.max_players, false);
        text(&mut a, "+server.worldsize", &self.world_size, false);
        text(&mut a, "+server.seed", &self.seed, false);
        text(&mut a, "+server.saveinterval", &self.save_interval, false);
        let custom = self.custom_args.trim();
        if !custom.is_empty() {
            a.extend(custom.split_whitespace().map(str::to_string));
        }
        a
    }

    /// Check the settings. An empty list means they are fine to save.
    pub fn problems(&self) -> Vec<ConfigProblem> {
        let mut out = Vec::new();
        let mut bad = |field: &'static str, message: &str| {
            out.push(ConfigProblem {
                field,
                message: message.to_string(),
            })
        };
        let number = |o: &TextOption| {
            !o.enabled || (!o.value.is_empty() && o.value.bytes().all(|b| b.is_ascii_digit()))
        };
        let in_range = |o: &TextOption, lo: u64, hi: u64| {
            !o.enabled || o.value.parse::<u64>().is_ok_and(|n| (lo..=hi).contains(&n))
        };
        let plain = |o: &TextOption| !o.enabled || !o.value.contains(['"', '\r', '\n']);

        if !plain(&self.hostname) {
            bad(
                "hostname",
                "The server name cannot contain quotes or line breaks.",
            );
        }
        if !plain(&self.identity)
            || (self.identity.enabled && self.identity.value.contains(['/', '\\', ':']))
        {
            bad(
                "identity",
                "The save folder name cannot contain quotes, slashes or a colon.",
            );
        }
        if !plain(&self.rcon_password) {
            bad(
                "rconPassword",
                "The RCON password cannot contain quotes or line breaks.",
            );
        }
        if !number(&self.port) || !in_range(&self.port, 1, 65535) {
            bad("port", "The server port must be a number from 1 to 65535.");
        }
        if !number(&self.rcon_port) || !in_range(&self.rcon_port, 1, 65535) {
            bad(
                "rconPort",
                "The RCON port must be a number from 1 to 65535.",
            );
        }
        if self.port.enabled && self.rcon_port.enabled && self.port.value == self.rcon_port.value {
            bad(
                "rconPort",
                "The RCON port must differ from the server port.",
            );
        }
        if !number(&self.max_players) || !in_range(&self.max_players, 1, 1000) {
            bad("maxPlayers", "Max players must be a number from 1 to 1000.");
        }
        if !number(&self.world_size) || !in_range(&self.world_size, 1000, 6000) {
            bad(
                "worldSize",
                "World size must be a number from 1000 to 6000.",
            );
        }
        if !number(&self.seed) {
            bad("seed", "The map seed must be a whole number.");
        }
        if !number(&self.save_interval) || !in_range(&self.save_interval, 10, 86_400) {
            bad(
                "saveInterval",
                "The save interval must be 10 seconds or more.",
            );
        }
        out
    }
}

/// A save-folder name made only of letters, digits, `-` and `_`.
pub fn sanitize_identity(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // "a (b)" would otherwise become "a__b"; keep single separators.
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '_' && out.ends_with('_') {
            continue;
        }
        out.push(c);
    }
    out.trim_matches('_').to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_arguments_match_what_v2_produced() {
        let a = ServerLaunchConfig::default().args(true).join(" ");
        assert_eq!(
            a,
            "-batchmode +server.secure 1 +server.hostname \"My Rust Server\" +server.identity \"rustyserver\" \
             +server.port 28015 +rcon.port 28016 +rcon.password \"changeme\" +rcon.web 1 \
             +server.maxplayers 50 +server.worldsize 3000 +server.saveinterval 300"
        );
    }

    #[test]
    fn unquoted_arguments_are_ready_for_a_process() {
        let a = ServerLaunchConfig::default().args(false);
        assert!(
            a.windows(2)
                .any(|w| w[0] == "+server.hostname" && w[1] == "My Rust Server")
        );
        assert!(!a.iter().any(|x| x.contains('"')));
    }

    #[test]
    fn disabled_and_empty_options_are_left_out_and_custom_args_are_split() {
        let mut c = ServerLaunchConfig::default();
        c.seed.enabled = true;
        c.rcon_web.enabled = false;
        c.hostname.value.clear();
        c.custom_args = "  +server.tickrate 30   +server.pve true ".into();
        let a = c.args(true);
        assert!(a.contains(&"+server.seed".to_string()));
        assert!(!a.contains(&"+rcon.web".to_string()));
        assert!(!a.contains(&"+server.hostname".to_string()));
        assert_eq!(
            &a[a.len() - 4..],
            ["+server.tickrate", "30", "+server.pve", "true"]
        );
    }

    #[test]
    fn v2_settings_files_load() {
        let v2 = r#"{
          "hostname": {"enabled": true, "value": "Old Server"},
          "identity": {"enabled": true, "value": "old"},
          "port": {"enabled": true, "value": "28015"},
          "rconPort": {"enabled": true, "value": "28016"},
          "rconPassword": {"enabled": true, "value": "hunter22"},
          "rconWeb": {"enabled": true},
          "maxPlayers": {"enabled": true, "value": "50"},
          "worldSize": {"enabled": true, "value": "3000"},
          "seed": {"enabled": false, "value": "12345"},
          "saveInterval": {"enabled": true, "value": "300"},
          "customArgs": ""
        }"#;
        let c: ServerLaunchConfig = serde_json::from_str(v2).unwrap();
        assert_eq!(c.hostname.value, "Old Server");
        assert_eq!(c.rcon_password.value, "hunter22");
        assert!(!c.seed.enabled);
        assert!(c.problems().is_empty());
        // Missing keys fall back to the defaults instead of failing.
        let partial: ServerLaunchConfig =
            serde_json::from_str(r#"{"hostname":{"enabled":true,"value":"x"}}"#).unwrap();
        assert_eq!(partial.port.value, "28015");
    }

    #[test]
    fn new_servers_get_a_random_password_and_a_safe_identity() {
        let a = ServerLaunchConfig::for_new_server("My Cool Server!");
        let b = ServerLaunchConfig::for_new_server("My Cool Server!");
        assert_eq!(a.identity.value, "my_cool_server");
        assert_eq!(a.hostname.value, "My Cool Server!");
        assert_eq!(
            ServerLaunchConfig::for_new_server("Latest server (2026-11-09)")
                .identity
                .value,
            "latest_server_2026-11-09"
        );
        assert_ne!(a.rcon_password.value, "changeme");
        assert_eq!(a.rcon_password.value.len(), 16);
        assert_ne!(
            a.rcon_password.value, b.rcon_password.value,
            "each server gets its own password"
        );
        assert!(
            a.rcon_password
                .value
                .bytes()
                .all(|c| PASSWORD_ALPHABET.contains(&c))
        );
    }

    #[test]
    fn bad_values_are_reported_in_plain_words() {
        let mut c = ServerLaunchConfig::default();
        c.port.value = "abc".into();
        c.rcon_port.value = "28015".into();
        c.hostname.value = "say \"hi\"".into();
        c.world_size.value = "100".into();
        let fields: Vec<_> = c.problems().iter().map(|p| p.field).collect();
        assert!(fields.contains(&"port"));
        assert!(fields.contains(&"hostname"));
        assert!(fields.contains(&"worldSize"));
        assert!(ServerLaunchConfig::default().problems().is_empty());
        // A disabled option is not checked.
        let mut d = ServerLaunchConfig::default();
        d.seed.value = "not a number".into();
        assert!(d.problems().is_empty());
    }

    #[test]
    fn identity_sanitising() {
        assert_eq!(sanitize_identity("  A b/c "), "a_b_c");
        assert_eq!(sanitize_identity("***"), "");
    }
}
