//! # Users Plugin - User Sessions
//!
//! Provides information about logged-in users and active sessions.
//! Shows active sessions, users, and per-user information via dynamic paths.

use std::fs;
use std::process::Command;
use std::sync::Arc;

use anyhow::Result;
use obsfs_core::{DynamicHandler, MetricProvider, MetricValue, Plugin, Registry};

// =============================================================================
// SESSION INFO
// =============================================================================

#[derive(Debug, Clone)]
struct UserSession {
    user: String,
    tty: String,
    host: String,
    login_time: String,
    #[allow(dead_code)]
    pid: u32,
}

// =============================================================================
// SESSION READER
// =============================================================================

struct SessionReader;

impl SessionReader {
    fn new() -> Self {
        Self
    }

    fn read_sessions(&self) -> Vec<UserSession> {
        // Try to use 'who' command first as it's more portable
        if let Ok(output) = Command::new("who").arg("-u").output() {
            if output.status.success() {
                return self.parse_who_output(&String::from_utf8_lossy(&output.stdout));
            }
        }

        // Fallback: try to read utmp directly
        self.read_utmp()
    }

    fn parse_who_output(&self, output: &str) -> Vec<UserSession> {
        let mut sessions = Vec::new();

        for line in output.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let user = parts[0].to_string();
                let tty = parts[1].to_string();

                // Parse login time and optional host
                let (host, login_time, pid) = if parts.len() >= 6 {
                    // Format: user tty date time (pid) or user tty host date time (pid)
                    if parts[2].starts_with('(') || parts[2].contains('.') {
                        // Has host
                        let host = parts[2].trim_matches(|c| c == '(' || c == ')').to_string();
                        let time = format!("{} {}", parts[3], parts[4]);
                        let pid = parts
                            .last()
                            .and_then(|s| s.trim_matches(|c| c == '(' || c == ')').parse().ok())
                            .unwrap_or(0);
                        (host, time, pid)
                    } else {
                        // No host
                        let time = format!("{} {}", parts[2], parts[3]);
                        let pid = parts
                            .get(4)
                            .and_then(|s| s.trim_matches(|c| c == '(' || c == ')').parse().ok())
                            .unwrap_or(0);
                        ("-".to_string(), time, pid)
                    }
                } else {
                    let time = parts.get(2..).map(|p| p.join(" ")).unwrap_or_default();
                    ("-".to_string(), time, 0)
                };

                sessions.push(UserSession {
                    user,
                    tty,
                    host,
                    login_time,
                    pid,
                });
            }
        }

        sessions
    }

    fn read_utmp(&self) -> Vec<UserSession> {
        // Try reading /var/run/utmp directly
        // This is a simplified parser - full utmp parsing is complex
        let mut sessions = Vec::new();

        if let Ok(output) = Command::new("who").output() {
            if output.status.success() {
                let content = String::from_utf8_lossy(&output.stdout);
                for line in content.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        sessions.push(UserSession {
                            user: parts[0].to_string(),
                            tty: parts.get(1).unwrap_or(&"?").to_string(),
                            host: parts
                                .get(4)
                                .unwrap_or(&"-")
                                .trim_matches('(')
                                .trim_matches(')')
                                .to_string(),
                            login_time: parts.get(2..4).map(|p| p.join(" ")).unwrap_or_default(),
                            pid: 0,
                        });
                    }
                }
            }
        }

        sessions
    }

    fn get_unique_users(&self) -> Vec<String> {
        let sessions = self.read_sessions();
        let mut users: Vec<String> = sessions.iter().map(|s| s.user.clone()).collect();
        users.sort();
        users.dedup();
        users
    }
}

// =============================================================================
// PROVIDERS
// =============================================================================

/// Provider for active user sessions.
pub struct ActiveSessionsProvider;

impl ActiveSessionsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ActiveSessionsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for ActiveSessionsProvider {
    fn path(&self) -> &str {
        "users/active"
    }

    fn collect(&self) -> Result<MetricValue> {
        let reader = SessionReader::new();
        let sessions = reader.read_sessions();

        let mut out = String::new();
        out.push_str("Active User Sessions\n");
        out.push_str(&"=".repeat(60));
        out.push_str("\n\n");

        if sessions.is_empty() {
            out.push_str("No active sessions\n");
        } else {
            out.push_str(&format!(
                "{:<12} {:<10} {:<18} {:<}\n",
                "USER", "TTY", "FROM", "LOGIN@"
            ));
            out.push_str(&"-".repeat(60));
            out.push('\n');

            for session in &sessions {
                let host = if session.host.is_empty() || session.host == "-" {
                    "-".to_string()
                } else {
                    session.host.clone()
                };

                out.push_str(&format!(
                    "{:<12} {:<10} {:<18} {:<}\n",
                    session.user, session.tty, host, session.login_time
                ));
            }

            let unique_users = sessions
                .iter()
                .map(|s| &s.user)
                .collect::<std::collections::HashSet<_>>()
                .len();

            out.push_str(&format!(
                "\nTotal: {} sessions ({} unique users)\n",
                sessions.len(),
                unique_users
            ));
        }

        Ok(MetricValue::Text(out))
    }
}

/// Provider for user summary.
pub struct UserSummaryProvider;

impl UserSummaryProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UserSummaryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricProvider for UserSummaryProvider {
    fn path(&self) -> &str {
        "users/summary"
    }

    fn collect(&self) -> Result<MetricValue> {
        let reader = SessionReader::new();
        let sessions = reader.read_sessions();

        let mut out = String::new();
        out.push_str("User Sessions Summary\n");
        out.push_str(&"=".repeat(50));
        out.push_str("\n\n");

        if sessions.is_empty() {
            out.push_str("No active sessions\n");
        } else {
            // Group by user
            let mut user_sessions: std::collections::HashMap<String, Vec<&UserSession>> =
                std::collections::HashMap::new();
            for session in &sessions {
                user_sessions
                    .entry(session.user.clone())
                    .or_default()
                    .push(session);
            }

            let mut users: Vec<_> = user_sessions.keys().collect();
            users.sort();

            for user in users {
                let user_sess = &user_sessions[user];
                out.push_str(&format!("{}:\n", user));
                for sess in user_sess {
                    out.push_str(&format!(
                        "  {} from {} ({})\n",
                        sess.tty, sess.host, sess.login_time
                    ));
                }
            }

            out.push_str(&format!(
                "\nTotal: {} users, {} sessions\n",
                user_sessions.len(),
                sessions.len()
            ));
        }

        Ok(MetricValue::Text(out))
    }
}

// =============================================================================
// USER INFO HANDLER
// =============================================================================

/// Dynamic handler for per-user information.
pub struct UserInfoHandler;

impl UserInfoHandler {
    pub fn new() -> Self {
        Self
    }

    fn get_user_info(&self, username: &str) -> Option<String> {
        let reader = SessionReader::new();
        let sessions: Vec<_> = reader
            .read_sessions()
            .into_iter()
            .filter(|s| s.user == username)
            .collect();

        if sessions.is_empty() {
            // Check if user exists but not logged in
            if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
                if !passwd
                    .lines()
                    .any(|l| l.starts_with(&format!("{}:", username)))
                {
                    return None;
                }
            }
        }

        let mut out = String::new();
        out.push_str(&format!("User: {}\n", username));
        out.push_str(&"=".repeat(50));
        out.push_str("\n\n");

        if sessions.is_empty() {
            out.push_str("Status: Not logged in\n");
        } else {
            out.push_str(&format!(
                "Status: Logged in ({} sessions)\n\n",
                sessions.len()
            ));

            out.push_str("Sessions:\n");
            for sess in &sessions {
                out.push_str(&format!(
                    "  {} from {} since {}\n",
                    sess.tty, sess.host, sess.login_time
                ));
            }
        }

        // Try to get user info from /etc/passwd
        if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
            for line in passwd.lines() {
                if line.starts_with(&format!("{}:", username)) {
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() >= 7 {
                        out.push_str(&format!("\nUID: {}\n", parts.get(2).unwrap_or(&"?")));
                        out.push_str(&format!("GID: {}\n", parts.get(3).unwrap_or(&"?")));
                        out.push_str(&format!("Home: {}\n", parts.get(5).unwrap_or(&"?")));
                        out.push_str(&format!("Shell: {}\n", parts.get(6).unwrap_or(&"?")));
                    }
                    break;
                }
            }
        }

        Some(out)
    }
}

impl Default for UserInfoHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicHandler for UserInfoHandler {
    fn prefix(&self) -> &str {
        "users"
    }

    fn list_entries(&self) -> Vec<String> {
        let reader = SessionReader::new();
        let mut entries = reader.get_unique_users();
        // Also add static entries
        entries.push("active".to_string());
        entries.push("summary".to_string());
        entries.sort();
        entries.dedup();
        entries
    }

    fn exists(&self, subpath: &str) -> bool {
        // Static paths
        if subpath == "active" || subpath == "summary" {
            return true;
        }

        // Check if user exists in /etc/passwd
        if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
            return passwd
                .lines()
                .any(|l| l.starts_with(&format!("{}:", subpath)));
        }

        false
    }

    fn read(&self, subpath: &str) -> Option<String> {
        // Static paths are handled by MetricProviders
        if subpath == "active" || subpath == "summary" {
            return None;
        }

        self.get_user_info(subpath)
    }
}

// =============================================================================
// USERS PLUGIN
// =============================================================================

/// Plugin that provides user session information.
pub struct UsersPlugin;

impl UsersPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for UsersPlugin {
    fn name(&self) -> &str {
        "users"
    }

    fn description(&self) -> &str {
        "User sessions at /obs/users/"
    }

    fn register(&self, registry: &mut Registry) -> Result<()> {
        registry
            .insert_provider(Arc::new(ActiveSessionsProvider::new()))
            .map_err(|e| anyhow::anyhow!(e))?;

        registry
            .insert_provider(Arc::new(UserSummaryProvider::new()))
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(())
    }

    fn dynamic_handlers(&self) -> Vec<Arc<dyn DynamicHandler>> {
        vec![Arc::new(UserInfoHandler::new())]
    }
}

impl Default for UsersPlugin {
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
        let plugin = UsersPlugin::new();
        assert_eq!(plugin.name(), "users");
        assert!(!plugin.description().is_empty());
        assert_eq!(plugin.dynamic_handlers().len(), 1);
    }

    #[test]
    fn test_session_reader() {
        let reader = SessionReader::new();
        // Just test that it doesn't panic
        let _ = reader.read_sessions();
        let _ = reader.get_unique_users();
    }

    #[test]
    fn test_handler_static_paths() {
        let handler = UserInfoHandler::new();
        assert!(handler.exists("active"));
        assert!(handler.exists("summary"));
    }
}
