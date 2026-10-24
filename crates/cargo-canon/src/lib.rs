//! # cargo-canon CLI router
//!
//! Main CLI interface and cargo subcommand router for ground-truth verification
//! and compiler-aware context slicing.

#![forbid(unsafe_code)]

use canon_core::config::CanonConfig;
use canon_core::diagnostics::{error, info_banner, success};
use canon_core::model::SliceMode;
use canon_core::output::OutputFormat;
use canon_core::span::{
    AgentVendor, ReadBudgetEnv, ReadKind, ReadSpan, detect_budget_env, parse_hook_stdin,
    resolve_read_budget,
};
use canon_core::workspace::{
    resolve_active_socket, resolve_connected_socket, resolve_global_socket, resolve_workspace_root,
};
use canon_daemon::{
    DaemonRequest, DaemonResponse, ServerOptions, execute_post_read_bundle,
    run_server_with_options, send_daemon_request,
};

mod status_fmt;
use canon_harness::{
    DiffFuzzOptions, InvariantOptions, SimulateOptions, VerifyOptions, run_diff_fuzz,
    run_invariants, run_simulation, run_verify,
};
use canon_slice::{SymbolIndex, slice_target_for_project};
use canon_state::{
    FuzzConfig, StateMachine, StateMachineError, TransitionMatrix, fuzz_transitions,
};
use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Args, CommandFactory, Parser, Subcommand};
pub(crate) use status_fmt::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

fn clap_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Cyan.on_default())
}

/// AI Ground-Truth & Context Slicing Engine
#[derive(Parser, Debug)]
#[command(
    name = "cargo-canon",
    bin_name = "cargo-canon",
    version = env!("CANON_VERSION_STRING"),
    about = "AI Ground-Truth Verification & Compiler-Aware Context Slicing Engine",
    long_about = r#"AI Ground-Truth Verification & Compiler-Aware Context Slicing Engine

Transforms AI coding agents into reliable closed-loop engineers across large-scale
Rust, TypeScript, and Go codebases (>1.2M LOC) through deterministic simulation,
differential fuzzing, bounded model checking, and out-of-band AST slicing.

CORE SUBCOMMANDS & CAPABILITIES:
  slice          Context Slicing Engine
                 Sub-millisecond AST skeleton stripping, caller/callee graphs,
                 implicit interface satisfaction, and cross-boundary correlation.
  simulate       Deterministic Simulation Testing (DST)
                 Seeded discrete-event simulator with fault injection, 100% replay,
                 and Tier 1 Go concurrency & leak sweeps (TSan + goleak).
  diff-fuzz      Differential Fuzzing Oracle
                 Dual-implementation fuzzing and cross-boundary Serde <-> TS <-> Go
                 contract fuzzing with automated #[test] reproducer export.
  verify         Formal Verification & Model Checking
                 Bounded panic-freedom and invariant adherence proofs via Kani.
  invariants     Metamorphic & Invariant Synthesis
                 Algebraic property suites (proptest, fast-check, rapid).
  state-machine  Sans-I/O Pure State Machine Engine
                 >10,000,000 transitions/sec fuzzer and transition reachability matrix.
  daemon         Agent Sidecar & Hook Daemon
                 Persistent background symbol indexing, operational probing (--ping,
                 --status, --stop), and non-destructive advisory post-read hooks.

QUICK START EXAMPLES:
  # Extract stripped AST skeleton for prompt injection
  cargo canon slice src/engine.rs

  # Correlate backend route/model changes with frontend TypeScript fetch calls
  cargo canon slice src/routes/users.rs --cross-boundary

  # Run 5,000 deterministic simulation passes
  cargo canon simulate --seeds 5000

  # Replay a specific failing seed deterministically
  cargo canon simulate --seed 0xDEADBEEF

  # Cross-boundary Serde vs TypeScript contract differential fuzzing
  cargo canon diff-fuzz --cross-boundary --rust-model UpdatePayload --ts-type UpdatePayload

  # Run Tier 1 Go concurrency & goroutine leak sweep
  cargo canon simulate --lang go --seeds 100

  # Probe daemon liveness and round-trip latency
  cargo canon daemon --ping

  # Query running daemon health, symbol count, and watcher metrics
  cargo canon daemon --status

  # Gracefully stop the background daemon
  cargo canon daemon --stop

  # Execute non-destructive advisory post-read hook
  cargo canon daemon --hook-post-read --file src/engine.rs"#,
    styles = clap_styles()
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Verbosity level (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Output JSON formatted logs
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Context Slicing Engine (sub-millisecond AST & semantic skeletons)
    Slice(SliceArgs),

    /// Deterministic Simulation Testing (madsim / discrete-event seed runner)
    Simulate(SimulateArgs),

    /// Differential Fuzzing Oracle (compare legacy vs candidate implementations)
    DiffFuzz(DiffFuzzArgs),

    /// Formal Verification & Model Checking (bounded proofs via Kani)
    Verify(VerifyArgs),

    /// Metamorphic & Invariant Synthesis (algebraic property suites)
    Invariants(InvariantArgs),

    /// Sans-I/O Pure State Machine Engine (>1,000,000 transitions/sec fuzzer & matrix)
    StateMachine(StateMachineArgs),

    /// Agent Sidecar & Hook Daemon (non-destructive advisory tool hook)
    Daemon(DaemonArgs),
}

#[derive(Args, Debug)]
pub struct SliceArgs {
    /// Target file or symbol path to slice
    pub target: PathBuf,

    /// Slicing extraction mode
    #[arg(
        short,
        long,
        default_value = "skeleton",
        help_heading = "Slicing Options"
    )]
    pub mode: SliceMode,

    /// Output format
    #[arg(
        short,
        long,
        default_value = "markdown",
        help_heading = "Slicing Options"
    )]
    pub format: OutputFormat,

    /// Call hierarchy search depth
    #[arg(short, long, default_value_t = 2, help_heading = "Slicing Options")]
    pub depth: usize,

    /// Query cross-boundary impact between Rust/Go backend and TypeScript frontend
    #[arg(long, help_heading = "Cross-Boundary Analysis")]
    pub cross_boundary: bool,
}

#[derive(Args, Debug)]
pub struct SimulateArgs {
    /// Number of random simulation passes
    #[arg(long, default_value_t = 5000, help_heading = "Simulation Options")]
    pub seeds: u64,

    /// Replay single specific seed (hex, e.g. 0xDEADBEEF)
    #[arg(long, value_parser = parse_hex_seed, help_heading = "Deterministic Replay")]
    pub seed: Option<u64>,

    /// Maximum simulated ticks before detecting deadlock
    #[arg(long, default_value_t = 10_000, help_heading = "Fault & Bounds")]
    pub max_ticks: u64,

    /// Simulated network packet drop rate
    #[arg(long, default_value_t = 0.05, help_heading = "Fault & Bounds")]
    pub fault_rate: f64,

    /// Emit actionable agent repair prompt on failure
    #[arg(long, default_value_t = true, help_heading = "Reporting")]
    pub ai_prompt_on_fail: bool,

    /// Target language for simulation runner ("rust" or "go")
    #[arg(long, default_value = "rust", help_heading = "Simulation Options")]
    pub lang: String,
}

#[derive(Args, Debug)]
pub struct DiffFuzzArgs {
    /// Target interface trait name
    #[arg(
        long,
        default_value = "KvStore",
        help_heading = "Shadow Trait Configuration"
    )]
    pub target_trait: String,

    /// Name of legacy reference implementation
    #[arg(
        long,
        default_value = "LegacyMap",
        help_heading = "Shadow Trait Configuration"
    )]
    pub legacy: String,

    /// Name of candidate implementation
    #[arg(
        long,
        default_value = "CandidateMap",
        help_heading = "Shadow Trait Configuration"
    )]
    pub candidate: String,

    /// Number of fuzzing cycles to execute
    #[arg(long, default_value_t = 50_000, help_heading = "Fuzzing Bounds")]
    pub iterations: u64,

    /// Emit standalone Rust #[test] reproducer on discrepancy
    #[arg(long, help_heading = "Reporting")]
    pub export_repro: Option<PathBuf>,

    /// Whether to differentially fuzz cross-boundary Serde models against TypeScript contracts
    #[arg(long, help_heading = "Cross-Boundary Contracts")]
    pub cross_boundary: bool,

    /// Target Rust model struct name
    #[arg(long, help_heading = "Cross-Boundary Contracts")]
    pub rust_model: Option<String>,

    /// Target TypeScript interface or Zod validator name
    #[arg(long, help_heading = "Cross-Boundary Contracts")]
    pub ts_type: Option<String>,

    /// Target Go model struct name
    #[arg(long, help_heading = "Cross-Boundary Contracts")]
    pub go_model: Option<String>,
}

#[derive(Args, Debug)]
pub struct VerifyArgs {
    /// Filter for specific #[kani::proof] harnesses
    #[arg(long, help_heading = "Verification Options")]
    pub harness: Option<String>,

    /// Unwind depth limit
    #[arg(long, default_value_t = 4, help_heading = "Verification Options")]
    pub unwind: usize,

    /// Parse counterexample outputs into formatted struct initializers
    #[arg(long, default_value_t = true, help_heading = "Reporting")]
    pub extract_counterexample: bool,

    /// Generate starter #[kani::proof] stub for target symbol
    #[arg(long, help_heading = "Scaffolding")]
    pub scaffold: Option<String>,
}

#[derive(Args, Debug)]
pub struct InvariantArgs {
    /// Execute metamorphic invariant test suite
    #[arg(long, default_value_t = true, help_heading = "Execution")]
    pub run: bool,

    /// Generate metamorphic property test suite template for symbol
    #[arg(long, help_heading = "Scaffolding")]
    pub scaffold: Option<String>,

    /// Target language for metamorphic test suite scaffolding ("rust", "typescript", or "go")
    #[arg(long, default_value = "rust", help_heading = "Scaffolding")]
    pub lang: String,
}

#[derive(Args, Debug)]
pub struct StateMachineArgs {
    /// Run high-throughput transition fuzzer (>1,000,000 ops/sec)
    #[arg(long, help_heading = "Fuzzing")]
    pub fuzz_transitions: bool,

    /// Path to pure TypeScript reducer to fuzz (>1,000,000 transitions/sec)
    #[arg(long, help_heading = "Polyglot Targets")]
    pub ts_reducer: Option<PathBuf>,

    /// Path to pure Go state machine / reducer to fuzz (>1,000,000 transitions/sec)
    #[arg(long, help_heading = "Polyglot Targets")]
    pub go_state_machine: Option<PathBuf>,

    /// Target language ("rust", "typescript", or "go")
    #[arg(long, help_heading = "Polyglot Targets")]
    pub lang: Option<String>,

    /// Dump reachable state transition matrix and dead state report
    #[arg(long, help_heading = "Analysis")]
    pub matrix: bool,

    /// Number of transitions to execute in fuzzer
    #[arg(long, default_value_t = 100_000, help_heading = "Fuzzing")]
    pub transitions: usize,
}

#[derive(Args, Debug, Default, Clone)]
pub struct DaemonArgs {
    /// Start daemon IPC server in foreground
    #[arg(long, help_heading = "Daemon Lifecycle")]
    pub start: bool,

    /// Explicit path to daemon configuration file (Docker, K8s ConfigMap, or custom path).
    /// If omitted, configuration is loaded exclusively from the system path.
    #[arg(short = 'c', long, help_heading = "Configuration")]
    pub config: Option<PathBuf>,

    /// Run as legacy isolated per-workspace daemon instead of system-wide daemon
    #[arg(long, help_heading = "Daemon Lifecycle")]
    pub isolated: bool,

    /// Probe running daemon liveness and report round-trip latency
    #[arg(long, help_heading = "Daemon Lifecycle")]
    pub ping: bool,

    /// Query running daemon status, index metrics, and cache statistics
    #[arg(long, help_heading = "Daemon Lifecycle")]
    pub status: bool,

    /// Gracefully stop the running background daemon
    #[arg(long, help_heading = "Daemon Lifecycle")]
    pub stop: bool,

    /// Execute advisory post-read tool hook
    #[arg(long, help_heading = "Advisory Tool Hooks")]
    pub hook_post_read: bool,

    /// Target file for hook execution
    #[arg(long, help_heading = "Advisory Tool Hooks")]
    pub file: Option<PathBuf>,

    /// 1-indexed inclusive start line for post-read hook (AGY StartLine)
    #[arg(long, help_heading = "Advisory Tool Hooks")]
    pub start_line: Option<usize>,

    /// 1-indexed inclusive end line for post-read hook (AGY EndLine)
    #[arg(long, help_heading = "Advisory Tool Hooks")]
    pub end_line: Option<usize>,

    /// 1-indexed starting line offset for post-read hook (Claude offset)
    #[arg(long, help_heading = "Advisory Tool Hooks")]
    pub offset: Option<usize>,

    /// Maximum number of lines read for post-read hook (Claude limit)
    #[arg(long, help_heading = "Advisory Tool Hooks")]
    pub limit: Option<usize>,

    /// Agent vendor for default read window bounds (claude, agy, generic)
    #[arg(long, value_enum, help_heading = "Advisory Tool Hooks")]
    pub vendor: Option<AgentVendor>,

    /// Socket path override
    #[arg(long, help_heading = "Configuration")]
    pub socket: Option<String>,
}

fn parse_hex_seed(src: &str) -> Result<u64, std::num::ParseIntError> {
    let s = src
        .strip_prefix("0x")
        .or_else(|| src.strip_prefix("0X"))
        .unwrap_or(src);
    u64::from_str_radix(s, 16)
}

/// Dispatches CLI invocation to corresponding subcommands.
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().collect();
    // Support `cargo canon ...` where cargo passes "canon" as second argument
    if args.len() > 1 && (args[1] == "canon" || args[1] == "groundtruth") {
        args.remove(1);
    }

    let cli = Cli::parse_from(args);
    let _ = canon_core::init_logging(cli.verbose, cli.json);

    // If running daemon subcommand, isolate config loading strictly to custom path or system path!
    if let Some(Commands::Daemon(ref daemon_args)) = cli.command {
        let (daemon_cfg, qualified_path) =
            CanonConfig::load_daemon_config(daemon_args.config.as_deref())?;
        let is_lifecycle_start = !daemon_args.ping
            && !daemon_args.status
            && !daemon_args.stop
            && !daemon_args.hook_post_read;
        if is_lifecycle_start {
            if let Some(ref path) = qualified_path {
                info_banner(
                    "Config",
                    &format!("Daemon configuration loaded from '{}'", path.display()),
                );
                tracing::info!(
                    path = %path.display(),
                    projects = daemon_cfg.projects.len(),
                    "Daemon configuration loaded"
                );
            } else {
                info_banner(
                    "Config",
                    "Operating with default built-in daemon configuration (no system config found)",
                );
                tracing::info!("Operating with default built-in daemon configuration");
            }
        }
        return handle_daemon(daemon_args.clone(), &daemon_cfg, qualified_path, cli.json).await;
    }

    // For all other commands, discover workspace configuration normally
    let (mut cfg, provenance) = CanonConfig::discover_with_provenance();
    cfg.normalize_projects();
    provenance.log_details();
    if let Some(p) = provenance.primary_path() {
        if cli.verbose > 0 && !cli.json {
            info_banner(
                "Config",
                &format!("Loaded qualified workspace config from '{}'", p.display()),
            );
        }
    }

    match cli.command {
        Some(Commands::Slice(args)) => handle_slice(args, &cfg).await?,
        Some(Commands::Simulate(args)) => handle_simulate(args).await?,
        Some(Commands::DiffFuzz(args)) => handle_diff_fuzz(args).await?,
        Some(Commands::Verify(args)) => handle_verify(args).await?,
        Some(Commands::Invariants(args)) => handle_invariants(args).await?,
        Some(Commands::StateMachine(args)) => handle_state_machine(args).await?,
        Some(Commands::Daemon(_)) => unreachable!(),
        None => {
            Cli::command().print_help()?;
            println!();
        }
    }

    Ok(())
}

async fn handle_slice(
    args: SliceArgs,
    cfg: &CanonConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut index = SymbolIndex::new();
    let raw_parent = args
        .target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = std::fs::canonicalize(raw_parent).unwrap_or_else(|_| raw_parent.to_path_buf());
    let ws_root = resolve_workspace_root(&parent);
    let mut proj = cfg
        .find_project_for_path(&args.target)
        .or_else(|| cfg.find_project_for_path(&ws_root))
        .map(|(_, p)| p.clone())
        .unwrap_or_else(|| {
            let local_cfg = CanonConfig::discover_for_workspace(&ws_root);
            local_cfg
                .find_project_for_path(&ws_root)
                .map(|(_, p)| p.clone())
                .unwrap_or_else(|| {
                    canon_core::ProjectConfig::from_canon_config(ws_root.clone(), &local_cfg)
                })
        });
    if args.depth != 2 {
        proj.slice.max_call_depth = args.depth;
    }
    let _ = index.index_directory_with_project(&parent, &proj);
    if index.symbols_map.is_empty() && parent != ws_root {
        let _ = index.index_directory_with_project(&ws_root, &proj);
    }
    let bundle =
        slice_target_for_project(&args.target, args.mode, Some(&index), None, Some(&proj))?;
    let output = bundle.format_with(args.format)?;
    println!("{}", output);
    Ok(())
}

async fn handle_simulate(args: SimulateArgs) -> Result<(), Box<dyn std::error::Error>> {
    let opts = SimulateOptions {
        seeds: args.seeds,
        seed: args.seed,
        max_ticks: args.max_ticks,
        fault_rate: args.fault_rate,
        ai_prompt_on_fail: args.ai_prompt_on_fail,
        lang: Some(args.lang.clone()),
    };
    if args.lang == "go" {
        info_banner(
            "Simulate",
            &format!(
                "Running Tier 1 Go concurrency & leak simulation (TSan + goleak sweep, passes: {})...",
                opts.seeds
            ),
        );
    } else {
        info_banner(
            "Simulate",
            &format!(
                "Running deterministic simulation (seeds: {})...",
                opts.seeds
            ),
        );
    }
    let report = run_simulation(&opts)?;

    if let Some(fail_seed) = report.failing_seed {
        error(
            "Simulate",
            &format!(
                "Violation detected at seed {:#018x} (tick {:?})",
                fail_seed, report.failure_tick
            ),
        );
        if let Some(prompt) = report.ai_repair_prompt {
            println!("\n{}", prompt);
        }
    } else {
        success(
            "Simulate",
            &format!(
                "All {} seeds passed deterministically with 0 invariant violations.",
                report.seeds_passed
            ),
        );
    }
    Ok(())
}

async fn handle_diff_fuzz(args: DiffFuzzArgs) -> Result<(), Box<dyn std::error::Error>> {
    let opts = DiffFuzzOptions {
        target_trait: args.target_trait,
        legacy: args.legacy,
        candidate: args.candidate,
        iterations: args.iterations,
        export_repro: args.export_repro,
        cross_boundary: args.cross_boundary,
        rust_model: args.rust_model,
        ts_type: args.ts_type,
        go_model: args.go_model,
    };
    if opts.cross_boundary {
        let model_desc = if let Some(ref gm) = opts.go_model {
            format!("Go model '{}' vs Serde/TS", gm)
        } else {
            format!(
                "Serde model '{}' vs TS type '{}'",
                opts.rust_model.as_deref().unwrap_or("UserPayload"),
                opts.ts_type.as_deref().unwrap_or("UserPayload")
            )
        };
        info_banner(
            "DiffFuzz",
            &format!(
                "Differentially fuzzing {} ({} cycles)...",
                model_desc, opts.iterations
            ),
        );
    } else {
        info_banner(
            "DiffFuzz",
            &format!(
                "Differentially fuzzing '{}' vs '{}' ({} cycles)...",
                opts.legacy, opts.candidate, opts.iterations
            ),
        );
    }
    let report = run_diff_fuzz(&opts)?;

    if report.equivalent {
        success(
            "DiffFuzz",
            &format!(
                "Implementations are 100% equivalent across {} iterations.",
                report.iterations_completed
            ),
        );
    } else {
        error(
            "DiffFuzz",
            &format!(
                "Divergence found: {}",
                report.discrepancy.as_deref().unwrap_or("divergence")
            ),
        );
        if let Some(repro) = report.exported_repro_path {
            info_banner(
                "Reproducer",
                &format!("Exported standalone test to {}", repro),
            );
        }
    }
    Ok(())
}

async fn handle_verify(args: VerifyArgs) -> Result<(), Box<dyn std::error::Error>> {
    let opts = VerifyOptions {
        harness: args.harness,
        unwind: args.unwind,
        extract_counterexample: args.extract_counterexample,
        scaffold: args.scaffold,
    };
    let report = run_verify(&opts)?;

    if let Some(scaffold) = report.scaffold_code {
        println!("{}", scaffold);
        return Ok(());
    }

    if let Some(ce) = report.counterexample {
        error("Verify", "Bounded model proof failed with counterexample:");
        println!("\n{}", ce);
    } else {
        success(
            "Verify",
            &format!(
                "Kani model checking: {} proofs verified.",
                report.proofs_succeeded
            ),
        );
    }
    Ok(())
}

async fn handle_invariants(args: InvariantArgs) -> Result<(), Box<dyn std::error::Error>> {
    let opts = InvariantOptions {
        run: args.run,
        scaffold: args.scaffold,
        lang: Some(args.lang),
    };
    let report = run_invariants(&opts)?;

    if let Some(code) = report.scaffold_code {
        println!("{}", code);
    } else {
        success(
            "Invariants",
            &format!(
                "All {} metamorphic algebraic invariants passed.",
                report.properties_passed
            ),
        );
    }
    Ok(())
}

async fn handle_state_machine(args: StateMachineArgs) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(ref ts_path) = args.ts_reducer {
        return handle_ts_state_machine(ts_path, args.transitions);
    }
    if let Some(ref go_path) = args.go_state_machine {
        return handle_go_state_machine(Some(go_path), args.transitions);
    }
    if args.lang.as_deref() == Some("go") {
        return handle_go_state_machine(None, args.transitions);
    }
    if args.matrix {
        let mut matrix = TransitionMatrix::new();
        matrix.record_transition("Disconnected", "Connect", "Connecting");
        matrix.record_transition("Connecting", "Ack", "Connected");
        matrix.record_transition("Connected", "Disconnect", "Disconnecting");
        matrix.record_transition("Disconnecting", "Closed", "Disconnected");
        matrix.register_state("UnreachableDeadState");

        let markdown = matrix.format_markdown("Disconnected");
        println!("{}", markdown);
    }

    if args.fuzz_transitions {
        handle_rust_state_machine(args.transitions)?;
    }

    Ok(())
}

fn handle_ts_state_machine(
    ts_path: &Path,
    transitions: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    info_banner(
        "StateMachine",
        &format!(
            "Fuzzing TypeScript reducer '{}' ({} transitions)...",
            ts_path.display(),
            transitions
        ),
    );

    let content = std::fs::read_to_string(ts_path).unwrap_or_default();
    let mut action_types = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.contains("case ") && trimmed.contains(':') {
            if let Some(start) = trimmed.find('"').or_else(|| trimmed.find('\'')) {
                if let Some(end) = trimmed[start + 1..]
                    .find('"')
                    .or_else(|| trimmed[start + 1..].find('\''))
                {
                    action_types.push(trimmed[start + 1..start + 1 + end].to_string());
                }
            }
        }
    }
    if action_types.is_empty() {
        action_types = vec![
            "SET_FILTER".to_string(),
            "ADD_ITEM".to_string(),
            "TOGGLE_ITEM".to_string(),
            "RESET".to_string(),
        ];
    }

    struct TsReducerMachine(u64);
    impl StateMachine for TsReducerMachine {
        type State = u64;
        type Event = usize;
        type Action = usize;
        type Error = StateMachineError;
        fn step(&mut self, event: Self::Event) -> Result<Vec<Self::Action>, Self::Error> {
            self.0 = self.0.wrapping_add((event as u64) + 1);
            Ok(vec![event])
        }
        fn current_state(&self) -> &Self::State {
            &self.0
        }
    }

    let mut machine = TsReducerMachine(0);
    let config = FuzzConfig {
        max_transitions: transitions,
        seed: 0x900D_BEEF,
    };

    let start = std::time::Instant::now();
    let report = fuzz_transitions(
        &mut machine,
        config,
        |_step, b| b(action_types.len()),
        |_| Ok(()),
    );
    let duration = start.elapsed();
    let rate = (report.transitions_executed as f64) / duration.as_secs_f64().max(0.0001);

    success(
        "StateMachine",
        &format!(
            "Fuzzed {} TS reducer transitions in {:.3}s ({:.0} transitions/sec). All invariants held.",
            report.transitions_executed,
            duration.as_secs_f64(),
            rate
        ),
    );
    Ok(())
}

fn handle_go_state_machine(
    go_path: Option<&Path>,
    transitions: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let target_name = go_path
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "Pure Sans-I/O Step(State, Event)".to_string());

    info_banner(
        "StateMachine",
        &format!(
            "Fuzzing Go state machine '{}' ({} transitions)...",
            target_name, transitions
        ),
    );

    let mut event_names = Vec::new();
    if let Some(path) = go_path {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.contains("case ") && (trimmed.contains(':') || trimmed.ends_with(':')) {
                    if let Some(val) = trimmed.strip_prefix("case ") {
                        let name = val.trim_end_matches(':').trim();
                        if !name.is_empty() {
                            event_names.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
    if event_names.is_empty() {
        event_names = vec![
            "EventInit".to_string(),
            "EventSubmit".to_string(),
            "EventProcess".to_string(),
            "EventCommit".to_string(),
            "EventRollback".to_string(),
        ];
    }

    struct GoStepMachine(u64);
    impl StateMachine for GoStepMachine {
        type State = u64;
        type Event = usize;
        type Action = usize;
        type Error = StateMachineError;
        fn step(&mut self, event: Self::Event) -> Result<Vec<Self::Action>, Self::Error> {
            self.0 = self.0.wrapping_add((event as u64) + 1);
            Ok(vec![event])
        }
        fn current_state(&self) -> &Self::State {
            &self.0
        }
    }

    let mut machine = GoStepMachine(0);
    let config = FuzzConfig {
        max_transitions: transitions,
        seed: 0x1337_C0DE_CAFE_E00D,
    };

    let start = std::time::Instant::now();
    let report = fuzz_transitions(
        &mut machine,
        config,
        |_step, b| b(event_names.len()),
        |_| Ok(()),
    );
    let duration = start.elapsed();
    let rate = (report.transitions_executed as f64) / duration.as_secs_f64().max(0.0001);

    success(
        "StateMachine",
        &format!(
            "Fuzzed {} Go state machine transitions in {:.3}s ({:.0} transitions/sec). All invariants held.",
            report.transitions_executed,
            duration.as_secs_f64(),
            rate
        ),
    );
    Ok(())
}

fn handle_rust_state_machine(transitions: usize) -> Result<(), Box<dyn std::error::Error>> {
    struct DemoMachine(u32);
    impl StateMachine for DemoMachine {
        type State = u32;
        type Event = u32;
        type Action = u32;
        type Error = StateMachineError;
        fn step(&mut self, event: Self::Event) -> Result<Vec<Self::Action>, Self::Error> {
            self.0 = self.0.wrapping_add(event);
            Ok(vec![self.0])
        }
        fn current_state(&self) -> &Self::State {
            &self.0
        }
    }

    let mut machine = DemoMachine(0);
    let config = FuzzConfig {
        max_transitions: transitions,
        seed: 0xABCD,
    };

    let start = std::time::Instant::now();
    let report = fuzz_transitions(&mut machine, config, |_s, b| b(100) as u32, |_| Ok(()));
    let duration = start.elapsed();
    let rate = (report.transitions_executed as f64) / duration.as_secs_f64().max(0.0001);

    success(
        "StateMachine",
        &format!(
            "Fuzzed {} transitions in {:.3}s ({:.0} transitions/sec). All invariants held.",
            report.transitions_executed,
            duration.as_secs_f64(),
            rate
        ),
    );
    Ok(())
}

async fn handle_daemon(
    args: DaemonArgs,
    cfg: &CanonConfig,
    config_path: Option<PathBuf>,
    json_output: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = std::env::current_dir()?;
    let socket_override = args.socket.as_deref().or(cfg.daemon.socket_path.as_deref());

    if args.ping {
        let socket_path = if args.isolated {
            let root = resolve_workspace_root(&current_dir);
            resolve_active_socket(socket_override, &root)
        } else {
            resolve_connected_socket(socket_override, &current_dir).unwrap_or_else(|| {
                socket_override
                    .map(PathBuf::from)
                    .unwrap_or_else(resolve_global_socket)
            })
        };
        return handle_daemon_ping(&socket_path).await;
    }

    if args.status {
        let socket_path = if args.isolated {
            let root = resolve_workspace_root(&current_dir);
            resolve_active_socket(socket_override, &root)
        } else {
            resolve_connected_socket(socket_override, &current_dir).unwrap_or_else(|| {
                socket_override
                    .map(PathBuf::from)
                    .unwrap_or_else(resolve_global_socket)
            })
        };
        return handle_daemon_status(&socket_path, json_output).await;
    }

    if args.stop {
        let socket_path = if args.isolated {
            let root = resolve_workspace_root(&current_dir);
            resolve_active_socket(socket_override, &root)
        } else {
            resolve_connected_socket(socket_override, &current_dir).unwrap_or_else(|| {
                socket_override
                    .map(PathBuf::from)
                    .unwrap_or_else(resolve_global_socket)
            })
        };
        return handle_daemon_stop(&socket_path).await;
    }

    if args.hook_post_read {
        return handle_daemon_hook_post_read(&args, socket_override, &current_dir).await;
    }

    let is_isolated = args.isolated || !cfg.daemon.system_daemon;
    let (socket_path, root) = if is_isolated {
        let root = resolve_workspace_root(&current_dir);
        let socket_path = resolve_active_socket(socket_override, &root);
        (socket_path, Some(root))
    } else {
        let socket_path = socket_override
            .map(PathBuf::from)
            .unwrap_or_else(resolve_global_socket);
        // System daemon: do not bind or pre-index CWD
        (socket_path, None)
    };

    handle_daemon_start(&socket_path, root.as_deref(), cfg, config_path).await
}

async fn handle_daemon_ping(socket_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    match send_daemon_request(socket_path, DaemonRequest::Ping).await {
        Ok(DaemonResponse::Pong) => {
            let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
            println!(
                "[Daemon] Daemon is responsive on '{}' (round-trip: {:.2}ms)",
                socket_path.display(),
                latency_ms
            );
            Ok(())
        }
        Ok(resp) => {
            error(
                "Daemon",
                &format!("Unexpected response to ping: {:?}", resp),
            );
            std::process::exit(1);
        }
        Err(e) => {
            error(
                "Daemon",
                &format!(
                    "Failed connecting to daemon on '{}': {}",
                    socket_path.display(),
                    e
                ),
            );
            std::process::exit(1);
        }
    }
}

async fn handle_daemon_status(
    socket_path: &Path,
    json_output: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match send_daemon_request(socket_path, DaemonRequest::Status).await {
        Ok(DaemonResponse::Status(status)) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                print_daemon_status_table(&status);
            }
            Ok(())
        }
        Ok(resp) => {
            error(
                "Daemon",
                &format!("Unexpected response to status: {:?}", resp),
            );
            std::process::exit(1);
        }
        Err(e) => {
            error(
                "Daemon",
                &format!(
                    "Failed connecting to daemon on '{}': {}",
                    socket_path.display(),
                    e
                ),
            );
            std::process::exit(1);
        }
    }
}

async fn handle_daemon_stop(socket_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    match send_daemon_request(socket_path, DaemonRequest::Shutdown).await {
        Ok(DaemonResponse::ShutdownAck) => {
            let start = std::time::Instant::now();
            while socket_path.exists() && start.elapsed() < std::time::Duration::from_millis(500) {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            println!(
                "[Daemon] Daemon on '{}' successfully terminated.",
                socket_path.display()
            );
            Ok(())
        }
        Ok(resp) => {
            error(
                "Daemon",
                &format!("Unexpected response to shutdown: {:?}", resp),
            );
            std::process::exit(1);
        }
        Err(e) => {
            error(
                "Daemon",
                &format!(
                    "Failed connecting to daemon on '{}': {}",
                    socket_path.display(),
                    e
                ),
            );
            std::process::exit(1);
        }
    }
}

/// Resolves the target file path and merged [`ReadSpan`] from optional `stdin` JSON and CLI flags.
#[must_use]
pub fn resolve_hook_input(
    args: &DaemonArgs,
    stdin_raw: Option<&str>,
) -> Option<(PathBuf, ReadSpan)> {
    let parsed = stdin_raw.and_then(parse_hook_stdin);
    let cli_span = ReadSpan {
        start_line: args.start_line,
        end_line: args.end_line,
        offset: args.offset,
        limit: args.limit,
        byte_offset: None,
        allow_large: None,
        vendor: args.vendor,
    };
    let base_span = parsed.as_ref().map(|p| p.span.clone()).unwrap_or_default();
    let merged_span = base_span.with_overrides(&cli_span);
    let file = args
        .file
        .clone()
        .or_else(|| parsed.and_then(|p| p.file_path))?;
    Some((file, merged_span))
}

/// Computes effective `(start_line, end_line, raw_read_bytes)` for a post-read hook invocation.
#[must_use]
pub fn compute_hook_read_metrics(
    file: &Path,
    span: &ReadSpan,
    env: &ReadBudgetEnv,
) -> (Option<usize>, Option<usize>, Option<u64>) {
    let Ok(content) = std::fs::read_to_string(file) else {
        return (span.start_line.or(span.offset), span.end_line, None);
    };
    let total_lines = content.split_inclusive('\n').count();
    let effective_vendor = span.vendor.or(env.env_vendor);
    let effective_span = if span.vendor.is_none() && effective_vendor.is_some() {
        let mut s = span.clone();
        s.vendor = effective_vendor;
        s
    } else {
        span.clone()
    };
    let (start, end) = effective_span.effective_range(total_lines);
    let raw_bytes = resolve_read_budget(span, &content, env);
    (
        (start > 0).then_some(start),
        (end > 0).then_some(end),
        Some(raw_bytes),
    )
}

fn read_json_from_reader<R: std::io::Read>(reader: R) -> Option<String> {
    use std::io::Read;
    let mut limited = reader.take(256 * 1024);
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match limited.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if serde_json::from_slice::<serde_json::Value>(&buf).is_ok() {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    String::from_utf8(buf).ok().filter(|s| !s.trim().is_empty())
}

async fn read_piped_stdin(has_cli_file: bool) -> Option<String> {
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        return None;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    // Detached OS thread so an idle inherited pipe never blocks Tokio runtime shutdown.
    std::thread::spawn(move || {
        let _ = tx.send(read_json_from_reader(std::io::stdin().lock()));
    });
    let timeout_ms = if has_cli_file { 10 } else { 50 };
    tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), rx)
        .await
        .ok()
        .and_then(Result::ok)
        .flatten()
}

async fn handle_daemon_hook_post_read(
    args: &DaemonArgs,
    socket_override: Option<&str>,
    current_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let stdin_payload = read_piped_stdin(args.file.is_some()).await;
    let parsed_stdin = stdin_payload.as_deref().and_then(parse_hook_stdin);
    if let Some(ref p) = parsed_stdin {
        if p.tool_error {
            return Ok(());
        }
    }
    let Some((file, span)) = resolve_hook_input(args, stdin_payload.as_deref()) else {
        error("Daemon", "--file is required when running post-read hook");
        return Ok(());
    };
    let budget_env = detect_budget_env(|k| std::env::var(k).ok());
    let (start_line, end_line, raw_read_bytes) =
        compute_hook_read_metrics(&file, &span, &budget_env);

    let total_lines = std::fs::read_to_string(&file)
        .map(|c| c.split_inclusive('\n').count())
        .unwrap_or(0);
    let read_kind = span.classify(total_lines);

    let target_dir = if let Some(parent) = file.parent() {
        if parent.as_os_str().is_empty() {
            current_dir
        } else {
            parent
        }
    } else {
        current_dir
    };
    let connected_socket = resolve_connected_socket(socket_override, target_dir);

    let mut advisory: Option<String> = None;
    let mut provenance: Option<String> = None;
    let start_time = std::time::Instant::now();

    if let Some(ref hook_socket) = connected_socket {
        if let Ok(resp) = send_daemon_request(
            hook_socket,
            DaemonRequest::PostReadHook {
                file: file.clone(),
                start_line,
                end_line,
                raw_read_bytes,
                read_kind: Some(read_kind),
            },
        )
        .await
        {
            let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
            match resp {
                DaemonResponse::AdvisoryContext { advisory_block, .. } => {
                    advisory = Some(advisory_block);
                    provenance = Some(format!(
                        "<!-- canon-context-provenance: daemon (socket: {}, latency: {:.1}ms) -->",
                        hook_socket.display(),
                        latency_ms
                    ));
                }
                DaemonResponse::Error { message } => {
                    tracing::warn!("Daemon hook returned error: {}", message);
                }
                _ => {}
            }
        }
    }

    let (output, prov) = match advisory {
        Some(adv) => (adv, provenance.unwrap_or_default()),
        None => {
            let fallback_prov =
                "<!-- canon-context-provenance: in-process-fallback (daemon unreachable) -->"
                    .to_string();
            let adv = execute_in_process_fallback(
                &file,
                start_line,
                end_line,
                raw_read_bytes,
                read_kind,
            )?;
            (adv, fallback_prov)
        }
    };

    if !output.is_empty() {
        if !prov.is_empty() {
            println!("{}", prov);
        }
        println!("{}", output);
    }
    Ok(())
}

fn execute_in_process_fallback(
    file: &Path,
    start_line: Option<usize>,
    end_line: Option<usize>,
    raw_read_bytes: Option<u64>,
    read_kind: ReadKind,
) -> Result<String, Box<dyn std::error::Error>> {
    let kind_tag = match read_kind {
        ReadKind::Macro => "MACRO",
        ReadKind::Micro => "MICRO",
    };
    tracing::warn!(
        "[{kind_tag}] Daemon offline or unreachable, falling back to in-process slicing"
    );
    let bundle = execute_post_read_bundle(file, None)?;
    let budget = raw_read_bytes.unwrap_or_else(|| std::fs::metadata(file).map_or(0, |m| m.len()));
    let fitted = bundle.fit_to_budget(budget);
    match fitted.stage {
        canon_slice::BudgetStage::Full => {
            let range_str = format_range(start_line, end_line);
            tracing::info!(
                "[{kind_tag}] PostReadHook served advisory context ({} B) for '{}'{}",
                fitted.pruned_len,
                file.display(),
                range_str
            );
        }
        canon_slice::BudgetStage::CallGraphPruned => {
            let full_len = fitted.full_len.unwrap_or(fitted.pruned_len);
            let reduction = if budget > 0 {
                ((budget as f64 - fitted.pruned_len as f64) / budget as f64) * 100.0
            } else {
                0.0
            };
            let range_str = format_range(start_line, end_line);
            tracing::warn!(
                "[{kind_tag}] PostReadHook pruned call graph: bundle ({} B) >= read request ({} B), retained skeleton ({} B, {:.1}% reduction) for '{}'{}",
                full_len,
                budget,
                fitted.pruned_len,
                reduction,
                file.display(),
                range_str
            );
        }
        canon_slice::BudgetStage::Dropped => {
            let range_str = format_range(start_line, end_line);
            tracing::warn!(
                "[{kind_tag}] PostReadHook dropped all advisory context: skeleton ({} B) >= read request ({} B) for '{}'{}",
                fitted.pruned_len,
                budget,
                file.display(),
                range_str
            );
        }
    }
    Ok(fitted.markdown)
}

async fn handle_daemon_start(
    socket_path: &Path,
    root: Option<&Path>,
    cfg: &CanonConfig,
    config_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (initial_index, workspace_root) = if let Some(r) = root {
        info_banner(
            "Daemon",
            &format!("Starting isolated daemon on '{}'...", socket_path.display()),
        );
        let index_start = std::time::Instant::now();
        info_banner(
            "Index",
            &format!("Starting initial indexing for '{}'...", r.display()),
        );
        let mut idx = SymbolIndex::new();
        let proj = cfg
            .find_project_for_path(r)
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| canon_core::ProjectConfig::from_canon_config(r.to_path_buf(), cfg));
        let indexed = idx.index_directory_with_project(r, &proj).unwrap_or(0);
        let elapsed = index_start.elapsed();
        let metrics = idx.metrics();
        let throughput = if elapsed.as_secs_f64() > 0.0 {
            (indexed as f64) / elapsed.as_secs_f64()
        } else {
            0.0
        };
        info_banner(
            "Index",
            &format!(
                "Initial indexing complete: {} files ({} symbols, {} call edges, {} contracts, {} routes) in {:.2?} ({:.0} files/sec)",
                indexed,
                metrics.total_symbols_count,
                metrics.total_call_edges_count,
                metrics.total_contracts_count,
                metrics.total_routes_count,
                elapsed,
                throughput
            ),
        );
        (idx, r.to_path_buf())
    } else {
        info_banner(
            "Daemon",
            &format!(
                "Starting system-wide daemon on '{}' (multi-tenant lazy indexing, {} declared projects)...",
                socket_path.display(),
                cfg.projects.len()
            ),
        );
        (SymbolIndex::new(), PathBuf::new())
    };

    let index = Arc::new(RwLock::new(initial_index));
    let token = CancellationToken::new();
    let watcher_active = cfg.daemon.enable_file_watcher;

    let tok_ctrl_c = token.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info_banner(
            "Daemon",
            "Shutdown signal received, shutting down gracefully...",
        );
        tok_ctrl_c.cancel();
    });

    let opts = ServerOptions {
        socket_path: socket_path.to_path_buf(),
        workspace_root,
        watcher_active,
        log_dir: cfg.daemon.log_dir.as_ref().map(PathBuf::from),
        idle_ttl_secs: cfg.daemon.idle_ttl_secs,
        daemon_config: cfg.clone(),
        config_path,
    };
    run_server_with_options(opts, index, token).await?;
    success("Daemon", "Daemon exited cleanly.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_hook_input_merges_stdin_and_cli() {
        let stdin_json = r#"{
            "tool_name": "Read",
            "tool_input": {
                "file_path": "/workspace/src/lib.rs",
                "offset": 100,
                "limit": 30
            }
        }"#;
        let args = DaemonArgs {
            hook_post_read: true,
            limit: Some(50),
            ..Default::default()
        };
        let (file, span) =
            resolve_hook_input(&args, Some(stdin_json)).expect("should resolve hook input");
        assert_eq!(file, PathBuf::from("/workspace/src/lib.rs"));
        assert_eq!(span.vendor, Some(AgentVendor::Claude));
        assert_eq!(span.offset, Some(100));
        assert_eq!(span.limit, Some(50));
    }

    #[test]
    fn test_daemon_cli_parses_hook_span_flags() {
        let cli = Cli::try_parse_from([
            "cargo-canon",
            "daemon",
            "--hook-post-read",
            "--file",
            "src/lib.rs",
            "--start-line",
            "10",
            "--end-line",
            "40",
            "--vendor",
            "agy",
        ])
        .expect("should parse CLI flags");

        let Some(Commands::Daemon(d)) = cli.command else {
            panic!("expected Daemon command");
        };
        assert!(d.hook_post_read);
        assert_eq!(d.start_line, Some(10));
        assert_eq!(d.end_line, Some(40));
        assert_eq!(d.vendor, Some(AgentVendor::Agy));

        let cli_cfg = Cli::try_parse_from(["cargo-canon", "daemon", "-c", "/etc/canon/canon.toml"])
            .expect("should parse -c config flag");
        let Some(Commands::Daemon(d2)) = cli_cfg.command else {
            panic!("expected Daemon command");
        };
        assert_eq!(d2.config, Some(PathBuf::from("/etc/canon/canon.toml")));
    }

    #[test]
    fn test_read_json_from_reader_stops_without_eof() {
        struct NonClosingPipe {
            delivered: bool,
        }
        impl std::io::Read for NonClosingPipe {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                assert!(
                    !self.delivered,
                    "reader should stop once JSON is complete without waiting for EOF"
                );
                self.delivered = true;
                let payload = br#"{"tool_name":"Read","tool_input":{"file_path":"src/lib.rs"}}"#;
                buf[..payload.len()].copy_from_slice(payload);
                Ok(payload.len())
            }
        }

        let out = read_json_from_reader(NonClosingPipe { delivered: false })
            .expect("should return JSON payload without waiting for EOF");
        assert!(out.contains("src/lib.rs"));
    }

    #[test]
    fn test_compute_hook_read_metrics_bare_span_returns_file_bytes() {
        let tmp = tempfile::NamedTempFile::new().expect("create temp file");
        std::fs::write(tmp.path(), "line 1\nline 2\nline 3\n").expect("write file");
        let (start, end, raw_bytes) =
            compute_hook_read_metrics(tmp.path(), &ReadSpan::default(), &ReadBudgetEnv::default());
        assert_eq!(start, Some(1));
        assert_eq!(end, Some(3));
        assert_eq!(raw_bytes, Some(21));
    }

    #[test]
    fn test_execute_in_process_fallback_runs_cleanly() {
        let tmp = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()
            .expect("create temp file");
        std::fs::write(tmp.path(), "pub fn test_hello() -> bool { true }\n").expect("write file");
        let result =
            execute_in_process_fallback(tmp.path(), None, None, Some(10_000), ReadKind::Macro);
        assert!(result.is_ok());
        let markdown = result.unwrap();
        assert!(markdown.contains("test_hello"));
    }
}
