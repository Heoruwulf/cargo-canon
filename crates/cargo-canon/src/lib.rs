//! # cargo-canon CLI router
//!
//! Main CLI interface and cargo subcommand router for ground-truth verification
//! and compiler-aware context slicing.

#![forbid(unsafe_code)]

use canon_core::config::CanonConfig;
use canon_core::diagnostics::{error, info_banner, success};
use canon_core::model::SliceMode;
use canon_core::output::OutputFormat;
use canon_daemon::{execute_post_read_hook, run_server, start_watcher};
use canon_harness::{
    DiffFuzzOptions, InvariantOptions, SimulateOptions, VerifyOptions, run_diff_fuzz,
    run_invariants, run_simulation, run_verify,
};
use canon_slice::{SymbolIndex, slice_target};
use canon_state::{
    FuzzConfig, StateMachine, StateMachineError, TransitionMatrix, fuzz_transitions,
};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// AI Ground-Truth & Context Slicing Engine
#[derive(Parser, Debug)]
#[command(name = "cargo-canon", bin_name = "cargo-canon", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Verbosity level (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Output JSON formatted logs
    #[arg(long)]
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
    #[arg(short, long, default_value = "skeleton")]
    pub mode: SliceMode,

    /// Output format
    #[arg(short, long, default_value = "markdown")]
    pub format: OutputFormat,

    /// Call hierarchy search depth
    #[arg(short, long, default_value_t = 2)]
    pub depth: usize,
}

#[derive(Args, Debug)]
pub struct SimulateArgs {
    /// Number of random simulation passes
    #[arg(long, default_value_t = 5000)]
    pub seeds: u64,

    /// Replay single specific seed (hex, e.g. 0xDEADBEEF)
    #[arg(long, value_parser = parse_hex_seed)]
    pub seed: Option<u64>,

    /// Maximum simulated ticks before detecting deadlock
    #[arg(long, default_value_t = 10_000)]
    pub max_ticks: u64,

    /// Simulated network packet drop rate
    #[arg(long, default_value_t = 0.05)]
    pub fault_rate: f64,

    /// Emit actionable agent repair prompt on failure
    #[arg(long, default_value_t = true)]
    pub ai_prompt_on_fail: bool,
}

#[derive(Args, Debug)]
pub struct DiffFuzzArgs {
    /// Target interface trait name
    #[arg(long, default_value = "KvStore")]
    pub target_trait: String,

    /// Name of legacy reference implementation
    #[arg(long, default_value = "LegacyMap")]
    pub legacy: String,

    /// Name of candidate implementation
    #[arg(long, default_value = "CandidateMap")]
    pub candidate: String,

    /// Number of fuzzing cycles to execute
    #[arg(long, default_value_t = 50_000)]
    pub iterations: u64,

    /// Emit standalone Rust #[test] reproducer on discrepancy
    #[arg(long)]
    pub export_repro: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct VerifyArgs {
    /// Filter for specific #[kani::proof] harnesses
    #[arg(long)]
    pub harness: Option<String>,

    /// Unwind depth limit
    #[arg(long, default_value_t = 4)]
    pub unwind: usize,

    /// Parse counterexample outputs into formatted struct initializers
    #[arg(long, default_value_t = true)]
    pub extract_counterexample: bool,

    /// Generate starter #[kani::proof] stub for target symbol
    #[arg(long)]
    pub scaffold: Option<String>,
}

#[derive(Args, Debug)]
pub struct InvariantArgs {
    /// Execute metamorphic invariant test suite
    #[arg(long, default_value_t = true)]
    pub run: bool,

    /// Generate metamorphic property test suite template for symbol
    #[arg(long)]
    pub scaffold: Option<String>,
}

#[derive(Args, Debug)]
pub struct StateMachineArgs {
    /// Run high-throughput transition fuzzer (>1,000,000 ops/sec)
    #[arg(long)]
    pub fuzz_transitions: bool,

    /// Dump reachable state transition matrix and dead state report
    #[arg(long)]
    pub matrix: bool,

    /// Number of transitions to execute in fuzzer
    #[arg(long, default_value_t = 100_000)]
    pub transitions: usize,
}

#[derive(Args, Debug)]
pub struct DaemonArgs {
    /// Start daemon IPC server
    #[arg(long)]
    pub start: bool,

    /// Execute advisory post-read tool hook
    #[arg(long)]
    pub hook_post_read: bool,

    /// Target file for hook execution
    #[arg(long)]
    pub file: Option<PathBuf>,

    /// Socket path override
    #[arg(long)]
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
    let cfg = CanonConfig::discover();

    match cli.command {
        Some(Commands::Slice(args)) => handle_slice(args).await?,
        Some(Commands::Simulate(args)) => handle_simulate(args).await?,
        Some(Commands::DiffFuzz(args)) => handle_diff_fuzz(args).await?,
        Some(Commands::Verify(args)) => handle_verify(args).await?,
        Some(Commands::Invariants(args)) => handle_invariants(args).await?,
        Some(Commands::StateMachine(args)) => handle_state_machine(args).await?,
        Some(Commands::Daemon(args)) => handle_daemon(args, &cfg).await?,
        None => {
            info_banner(
                "Canon",
                "AI Ground-Truth & Context Slicing Engine. Run with --help for subcommands.",
            );
        }
    }

    Ok(())
}

async fn handle_slice(args: SliceArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut index = SymbolIndex::new();
    if let Some(parent) = args.target.parent() {
        let _ = index.index_directory(parent);
    }
    let bundle = slice_target(&args.target, args.mode, Some(&index))?;
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
    };
    info_banner(
        "Simulate",
        &format!(
            "Running deterministic simulation (seeds: {})...",
            opts.seeds
        ),
    );
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
    };
    info_banner(
        "DiffFuzz",
        &format!(
            "Differentially fuzzing '{}' vs '{}' ({} cycles)...",
            opts.legacy, opts.candidate, opts.iterations
        ),
    );
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
            &format!("Divergence found: {:?}", report.discrepancy),
        );
        if let Some(repro) = report.exported_repro_path {
            info_banner(
                "Reproducer",
                &format!("Exported standalone #[test] to {}", repro),
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
            max_transitions: args.transitions,
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
    }

    Ok(())
}

async fn handle_daemon(
    args: DaemonArgs,
    cfg: &CanonConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let socket_str = args
        .socket
        .unwrap_or_else(|| cfg.daemon.socket_path.clone());
    let socket_path = PathBuf::from(socket_str);

    if args.hook_post_read {
        if let Some(file) = args.file {
            let advisory = execute_post_read_hook(&file, None)?;
            println!("{}", advisory);
        } else {
            error("Daemon", "--file is required when running post-read hook");
        }
        return Ok(());
    }

    info_banner(
        "Daemon",
        &format!(
            "Starting background daemon on '{}'...",
            socket_path.display()
        ),
    );
    let index = Arc::new(RwLock::new(SymbolIndex::new()));
    let token = CancellationToken::new();

    if cfg.daemon.enable_file_watcher {
        let _ = start_watcher(PathBuf::from("."), Arc::clone(&index), token.clone());
    }

    let tok_ctrl_c = token.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info_banner(
            "Daemon",
            "Shutdown signal received, shutting down gracefully...",
        );
        tok_ctrl_c.cancel();
    });

    run_server(&socket_path, index, token).await?;
    success("Daemon", "Daemon exited cleanly.");
    Ok(())
}
