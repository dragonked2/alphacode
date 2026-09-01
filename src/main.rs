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
    const WINDOWS_MAIN_STACK_SIZE: usize = 8 * 1024 * 1024;
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
    // TLS sockets; the ~5-20ms init cost of aws_lc_rs can dominate startup
    // for those subcommands, which are expected to return quickly.
    if let Some(source) = cli_launch_hint_source_invocation() {
        return alphacode::setup_hints::run_setup_hotkey(false, false, false, Some(&source));
    }

    if is_macos_hotkey_listener_invocation() {
        return alphacode::setup_hints::run_macos_hotkey_listener_main_thread();
    }

    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let runtime = tokio::runtime::Builder::new_multi_thread()
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
