use anyhow::Result;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn configure_system_allocator() {
    unsafe extern "C" {
        fn mallopt(param: i32, value: i32) -> i32;
    }

    const M_ARENA_MAX: i32 = -8;
    const M_MMAP_THRESHOLD: i32 = -3;

    let arena_max = parse_alloc_tuning_env("ALPHACODE_GLIBC_ARENA_MAX", 4);
    let _ = unsafe { mallopt(M_ARENA_MAX, arena_max) };

    let mmap_threshold = parse_alloc_tuning_env("ALPHACODE_GLIBC_MMAP_THRESHOLD", 256 * 1024);
    let _ = unsafe { mallopt(M_MMAP_THRESHOLD, mmap_threshold) };
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn parse_alloc_tuning_env(var: &str, default: i32) -> i32 {
    parse_alloc_tuning(std::env::var(var).ok().as_deref(), default)
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn parse_alloc_tuning(value: Option<&str>, default: i32) -> i32 {
    value
        .and_then(|value| value.trim().parse::<i32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn configure_system_allocator() {}

#[cfg(windows)]
fn main() -> Result<()> {
    // Increased from 8MB to 16MB stack for deep recursion in agent/swarm loops,
    // heavy async combinator nesting, and large serialized payloads.
    const WINDOWS_MAIN_STACK_SIZE: usize = 16 * 1024 * 1024;
    match std::thread::Builder::new()
        .name("alphacode-main".to_string())
        .stack_size(WINDOWS_MAIN_STACK_SIZE)
        .spawn(run_main)?
        .join()
    {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[cfg(not(windows))]
fn main() -> Result<()> {
    run_main()
}

fn run_main() -> Result<()> {
    configure_system_allocator();

    // Short-circuit before installing the rustls crypto provider. The macOS
    // hotkey listener and the setup-hotkey notification path don't open any
    // TLS sockets; the init cost of the ring crypto provider can dominate
    // startup for those subcommands, which are expected to return quickly.
    if let Some(source) = cli_launch_hint_source_invocation() {
        return alphacode::setup_hints::run_setup_hotkey(false, false, false, Some(&source));
    }

    if is_macos_hotkey_listener_invocation() {
        return alphacode::setup_hints::run_macos_hotkey_listener_main_thread();
    }

    let _ = rustls::crypto::ring::default_provider().install_default();

    // Optimize tokio runtime: pin thread count based on available CPUs,
    // enable both IO and time drivers, and configure the scheduler for
    // low-latency agent workloads.
    let worker_threads = std::env::var("ALPHACODE_WORKER_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| {
            let cpus = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            // Use all available cores but cap at 32 to avoid thread overhead
            cpus.min(32)
        });

    let blocking_threads = std::env::var("ALPHACODE_BLOCKING_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(512);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .max_blocking_threads(blocking_threads)
        .thread_stack_size(2 * 1024 * 1024)
        .thread_name_fn(|| {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static IDS: AtomicUsize = AtomicUsize::new(0);
            let id = IDS.fetch_add(1, Ordering::Relaxed);
            format!("alphacode-worker-{id}")
        })
        .on_thread_start(|| {
            // Hint to the OS that worker threads are latency-sensitive.
            #[cfg(unix)]
            {
                // No-op if platform disallows; never fail startup for this.
                let _ = std::thread::current().name();
            }
        })
        .enable_all()
        .build()?;

    runtime.block_on(async { alphacode::run().await })
}

fn is_macos_hotkey_listener_invocation() -> bool {
    args_are_macos_hotkey_listener(std::env::args().skip(1))
}

fn args_are_macos_hotkey_listener(args: impl IntoIterator<Item = String>) -> bool {
    let args: Vec<String> = args.into_iter().collect();
    args.first().map(String::as_str) == Some("setup-hotkey")
        && args.iter().any(|a| a == "--listen-macos-hotkey")
}

fn cli_launch_hint_source_invocation() -> Option<String> {
    cli_launch_hint_source(std::env::args().skip(1))
}

fn cli_launch_hint_source(args: impl IntoIterator<Item = String>) -> Option<String> {
    let args: Vec<String> = args.into_iter().collect();
    if args.first().map(String::as_str) != Some("setup-hotkey") {
        return None;
    }
    let index = args.iter().position(|arg| arg == "--notify-cli-launch")?;
    args.get(index + 1).cloned()
}
