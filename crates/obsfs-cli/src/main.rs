//! ObsFS CLI - Command-line interface for the ObsFS observability filesystem.

mod banner;

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use obsfs_core::{
    Config, LogFormat, LogOutput, LoggingConfig, MetricProvider, MetricValue, Registry,
};
use obsfs_fuse::ObsFs;
use obsfs_plugins::{
    ConnectionsPlugin, DockerPlugin, HealthPlugin, Plugin, ProcSysPlugin, ProcessInfoPlugin,
    SensorsPlugin, ServicesPlugin, UsersPlugin,
};

use banner::StartupBanner;

#[derive(Parser, Debug)]
#[command(
    name = "obsfs",
    author,
    version,
    about = "Observe everything as files",
    long_about = "ObsFS mounts a virtual filesystem where each file returns an observability metric.\n\
                  Use cat, grep, watch, and other Unix tools to query metrics.\n\n\
                  Example:\n  \
                  sudo obsfs mount /obs\n  \
                  cat /obs/system/cpu/usage\n  \
                  watch -n 1 cat /obs/system/memory/available"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Mount the ObsFS filesystem
    Mount {
        /// Mount point path
        #[arg(default_value = "/obs")]
        path: PathBuf,

        /// Run as a daemon (background process)
        #[arg(long, short = 'd')]
        daemon: bool,

        /// Path to configuration file
        #[arg(long, short = 'c')]
        config: Option<PathBuf>,

        /// Allow other users to access the mount
        #[arg(long)]
        allow_other: bool,
    },

    /// Unmount the ObsFS filesystem
    Unmount {
        /// Mount point path
        #[arg(default_value = "/obs")]
        path: PathBuf,
    },

    /// Show daemon status
    Status,

    /// Show version information
    Version,
}

fn main() {
    let cli = Cli::parse();

    // For mount command, we need to load config first to get logging settings
    let logging_config = match &cli.command {
        Commands::Mount { config, .. } => load_logging_config(config.as_ref()),
        _ => LoggingConfig::default(),
    };

    init_logging(&logging_config);

    let result = match cli.command {
        Commands::Mount {
            path,
            daemon,
            config,
            allow_other,
        } => cmd_mount(path, daemon, config, allow_other),
        Commands::Unmount { path } => cmd_unmount(path),
        Commands::Status => cmd_status(),
        Commands::Version => cmd_version(),
    };

    if let Err(e) = result {
        tracing::error!("{:#}", e);
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}

/// Load logging configuration from config file, falling back to defaults.
fn load_logging_config(config_path: Option<&PathBuf>) -> LoggingConfig {
    let config = match config_path {
        Some(p) => Config::load_from(p).ok(),
        None => Config::load_default().ok(),
    };

    config.map(|c| c.logging).unwrap_or_default()
}

fn init_logging(config: &LoggingConfig) {
    // RUST_LOG env var takes precedence over config file
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.level.as_filter_str()));

    // Build the subscriber based on output destination and format
    match &config.output {
        LogOutput::Stdout => init_logging_to_stdout(config, filter),
        LogOutput::Stderr => init_logging_to_stderr(config, filter),
        LogOutput::File(path) => init_logging_to_file(config, filter, path),
    }
}

fn init_logging_to_stdout(config: &LoggingConfig, filter: EnvFilter) {
    match config.format {
        LogFormat::Json => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter)
                .with_target(config.show_target)
                .with_file(config.show_location)
                .with_line_number(config.show_location)
                .with_writer(std::io::stdout)
                .init();
        }
        LogFormat::Pretty => {
            tracing_subscriber::fmt()
                .pretty()
                .with_env_filter(filter)
                .with_target(config.show_target)
                .with_file(config.show_location)
                .with_line_number(config.show_location)
                .with_writer(std::io::stdout)
                .init();
        }
        LogFormat::Compact => {
            tracing_subscriber::fmt()
                .compact()
                .with_env_filter(filter)
                .with_target(config.show_target)
                .with_file(config.show_location)
                .with_line_number(config.show_location)
                .with_writer(std::io::stdout)
                .init();
        }
    }
}

// =============================================================================
// PLUGIN STATUS PROVIDER
// =============================================================================

/// Tracks and provides plugin registration status.
struct PluginStatusProvider {
    statuses: Arc<Mutex<HashMap<String, PluginStatus>>>,
}

#[derive(Clone, Debug)]
struct PluginStatus {
    success: bool,
    error: Option<String>,
}

impl PluginStatusProvider {
    fn new(statuses: Arc<Mutex<HashMap<String, PluginStatus>>>) -> Self {
        Self { statuses }
    }
}

impl MetricProvider for PluginStatusProvider {
    fn path(&self) -> &str {
        "_meta/plugins"
    }

    fn collect(&self) -> Result<MetricValue, anyhow::Error> {
        let statuses = self.statuses.lock().unwrap();

        let mut output = String::new();

        // Sort by plugin name for consistent output
        let mut plugins: Vec<_> = statuses.iter().collect();
        plugins.sort_by_key(|(name, _)| *name);

        for (name, status) in plugins {
            if status.success {
                output.push_str(&format!("{}: ok\n", name));
            } else {
                match &status.error {
                    Some(error) => output.push_str(&format!("{}: failed ({})\n", name, error)),
                    None => output.push_str(&format!("{}: failed\n", name)),
                }
            }
        }

        Ok(MetricValue::Text(output))
    }
}

fn init_logging_to_stderr(config: &LoggingConfig, filter: EnvFilter) {
    match config.format {
        LogFormat::Json => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter)
                .with_target(config.show_target)
                .with_file(config.show_location)
                .with_line_number(config.show_location)
                .init(); // stderr is default
        }
        LogFormat::Pretty => {
            tracing_subscriber::fmt()
                .pretty()
                .with_env_filter(filter)
                .with_target(config.show_target)
                .with_file(config.show_location)
                .with_line_number(config.show_location)
                .init();
        }
        LogFormat::Compact => {
            tracing_subscriber::fmt()
                .compact()
                .with_env_filter(filter)
                .with_target(config.show_target)
                .with_file(config.show_location)
                .with_line_number(config.show_location)
                .init();
        }
    }
}

fn init_logging_to_file(config: &LoggingConfig, filter: EnvFilter, path: &std::path::Path) {
    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Setup daily rolling file appender
    let file_appender = tracing_appender::rolling::daily(
        path.parent().unwrap_or_else(|| std::path::Path::new(".")),
        path.file_name().unwrap_or_default(),
    );
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Store the guard to prevent the worker thread from being dropped
    // This guard will be kept alive for the lifetime of the program
    let _guard = Box::leak(Box::new(_guard));

    match config.format {
        LogFormat::Json => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter)
                .with_target(config.show_target)
                .with_file(config.show_location)
                .with_line_number(config.show_location)
                .with_writer(non_blocking)
                .init();
        }
        LogFormat::Pretty => {
            tracing_subscriber::fmt()
                .pretty()
                .with_env_filter(filter)
                .with_target(config.show_target)
                .with_file(config.show_location)
                .with_line_number(config.show_location)
                .with_writer(non_blocking)
                .init();
        }
        LogFormat::Compact => {
            tracing_subscriber::fmt()
                .compact()
                .with_env_filter(filter)
                .with_target(config.show_target)
                .with_file(config.show_location)
                .with_line_number(config.show_location)
                .with_writer(non_blocking)
                .init();
        }
    }
}

fn cmd_mount(
    path: PathBuf,
    daemon: bool,
    config_path: Option<PathBuf>,
    allow_other: bool,
) -> Result<()> {
    tracing::info!(?path, ?daemon, "Starting ObsFS mount");

    // Load and validate config BEFORE banner (may have errors)
    let config = match config_path {
        Some(p) => {
            Config::load_from(&p).with_context(|| format!("failed to load config from {:?}", p))?
        }
        None => Config::load_default().context("failed to load default config")?,
    };

    if let Err(errors) = config.validate() {
        for error in &errors {
            tracing::error!("{}", error);
            eprintln!("Config error: {}", error);
        }
        anyhow::bail!("configuration validation failed");
    }

    // Check mount point BEFORE banner (may need user input)
    if !path.exists() {
        eprint!("Mount point {:?} does not exist. Create it? [y/N] ", path);
        std::io::stderr().flush().ok();

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .context("failed to read user input")?;

        if input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes") {
            std::fs::create_dir_all(&path)
                .with_context(|| format!("failed to create directory {:?}", path))?;
        } else {
            anyhow::bail!("mount point {:?} does not exist", path);
        }
    }

    if !path.is_dir() {
        anyhow::bail!("mount point {:?} is not a directory", path);
    }

    // NOW start the banner (all user interaction is done)
    let mut banner = StartupBanner::new();
    banner.print_header(env!("CARGO_PKG_VERSION"));

    let mut registry = Registry::new();

    // Initialize all plugins
    let plugins: Vec<Box<dyn Plugin>> = vec![
        // Core system metrics
        Box::new(ProcSysPlugin::new()),
        Box::new(HealthPlugin::new()),
        Box::new(ProcessInfoPlugin::new()),
        // Infrastructure
        Box::new(ConnectionsPlugin::new()),
        Box::new(ServicesPlugin::new()),
        Box::new(SensorsPlugin::new()),
        Box::new(UsersPlugin::new()),
        // Containers
        Box::new(DockerPlugin::new()),
    ];

    // Track plugin registration status for /_meta/plugins endpoint
    let plugin_statuses: Arc<Mutex<HashMap<String, PluginStatus>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // Register static providers from all plugins (continue on failure)
    for plugin in &plugins {
        let result = plugin.register(&mut registry);
        match &result {
            Ok(_) => {
                banner.print_status(plugin.name(), true);
                tracing::info!(plugin = plugin.name(), "Registered plugin");
                plugin_statuses.lock().unwrap().insert(
                    plugin.name().to_string(),
                    PluginStatus {
                        success: true,
                        error: None,
                    },
                );
            }
            Err(e) => {
                banner.print_status(plugin.name(), false);
                let error_msg = format!("{:#}", e);
                tracing::warn!(plugin = plugin.name(), error = %error_msg, "Plugin failed to register, continuing without it");
                plugin_statuses.lock().unwrap().insert(
                    plugin.name().to_string(),
                    PluginStatus {
                        success: false,
                        error: Some(error_msg),
                    },
                );
            }
        }
    }

    // Register the plugin status provider
    let status_provider = PluginStatusProvider::new(plugin_statuses.clone());
    if let Err(e) = registry.insert_provider(Arc::new(status_provider)) {
        tracing::warn!("Failed to register plugin status provider: {}", e);
    }

    let mut fs = ObsFs::new(registry);

    // Register dynamic handlers from all plugins
    for plugin in &plugins {
        for handler in plugin.dynamic_handlers() {
            tracing::info!(
                plugin = plugin.name(),
                prefix = handler.prefix(),
                "Registered dynamic handler"
            );
            fs.register_dynamic_handler(handler);
        }
    }

    let mut options = vec![
        fuser::MountOption::RO,
        fuser::MountOption::FSName("obsfs".to_string()),
        fuser::MountOption::Subtype("obsfs".to_string()),
    ];

    if allow_other || config.mount.allow_other {
        options.push(fuser::MountOption::AllowOther);
    }

    // Write PID file BEFORE daemonizing (while we still have stdio)
    if daemon {
        let pid = std::process::id();
        let pid_file = "/var/run/obsfs.pid";
        if let Err(e) = std::fs::write(pid_file, pid.to_string()) {
            tracing::warn!("Failed to write PID file {}: {}", pid_file, e);
            eprintln!("Warning: failed to write PID file {}: {}", pid_file, e);
        }
    }

    tracing::info!(?path, "Mounting ObsFS");

    // Daemonize AFTER logging is set up and BEFORE mounting FUSE
    if daemon {
        let pid = std::process::id();
        tracing::info!("Daemonizing with PID {}", pid);

        // Daemonize the process: chdir to / and redirect stdio to /dev/null
        nix::unistd::daemon(false, false).context("failed to daemonize")?;

        tracing::info!("Successfully daemonized");
    }

    let session = fuser::spawn_mount2(fs, &path, &options)
        .with_context(|| format!("failed to mount filesystem at {:?}", path))?;

    banner.print_mount(&path.display().to_string());
    banner.print_ready();

    let running = Arc::new(AtomicBool::new(true));
    let daemon_mode = daemon;

    // Register SIGINT (Ctrl+C) for graceful shutdown
    let r = running.clone();
    unsafe {
        signal_hook::low_level::register(signal_hook::consts::SIGINT, move || {
            if !daemon_mode {
                banner::print_shutdown();
            }
            r.store(false, Ordering::SeqCst);
        })?;
    }

    // Register SIGTERM (kill) for graceful shutdown
    let r = running.clone();
    unsafe {
        signal_hook::low_level::register(signal_hook::consts::SIGTERM, move || {
            r.store(false, Ordering::SeqCst);
        })?;
    }

    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    drop(session);

    tracing::info!("ObsFS unmounted");
    banner::print_unmounted();
    Ok(())
}

fn cmd_unmount(path: PathBuf) -> Result<()> {
    tracing::info!(?path, "Unmounting ObsFS");

    let status = std::process::Command::new("fusermount")
        .arg("-u")
        .arg(&path)
        .status()
        .context("failed to run fusermount")?;

    if status.success() {
        println!("ObsFS unmounted from {:?}", path);
        return Ok(());
    }

    let status = std::process::Command::new("umount")
        .arg(&path)
        .status()
        .context("failed to run umount")?;

    if status.success() {
        println!("ObsFS unmounted from {:?}", path);
        Ok(())
    } else {
        anyhow::bail!("failed to unmount {:?}", path)
    }
}

fn cmd_status() -> Result<()> {
    let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();

    let obsfs_mounts: Vec<&str> = mounts
        .lines()
        .filter(|line| line.contains("obsfs") || line.contains("fuse.obsfs"))
        .collect();

    if obsfs_mounts.is_empty() {
        println!("ObsFS is not mounted");
    } else {
        println!("ObsFS mounts:");
        for mount in obsfs_mounts {
            let parts: Vec<&str> = mount.split_whitespace().collect();
            if parts.len() >= 2 {
                println!("  {} -> {}", parts[0], parts[1]);
            }
        }
    }

    Ok(())
}

fn cmd_version() -> Result<()> {
    const CYAN: &str = "\x1b[36m";
    const BOLD: &str = "\x1b[1m";
    const DIM: &str = "\x1b[2m";
    const RESET: &str = "\x1b[0m";

    println!();
    println!(
        "{CYAN}   /ObsFS{RESET}       {BOLD}v{}{RESET}",
        env!("CARGO_PKG_VERSION")
    );
    println!("{CYAN}   ├──┬──●{RESET}      {DIM}Observe everything as files.{RESET}");
    println!("{CYAN}   │  └──●{RESET}");
    println!(
        "{CYAN}   ├──●{RESET}         {DIM}Repository:{RESET} {}",
        env!("CARGO_PKG_REPOSITORY")
    );
    println!("{CYAN}   └──┬──●{RESET}      {DIM}License:{RESET}    MIT");
    println!("{CYAN}      └──●{RESET}");
    println!();
    Ok(())
}
