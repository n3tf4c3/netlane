fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
fn run() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 2 && args[0] == "--validate" {
        let file = netlane_core::persistence::PolicyFile::new(&args[1]);
        let loaded = file.load()?;
        println!("{}", serde_json::to_string_pretty(&loaded.policies)?);
        return Ok(());
    }
    if args.len() == 2 && args[0] == "--plan" {
        let loaded = netlane_core::persistence::PolicyFile::new(&args[1]).load()?;
        let snapshot = netlane_windows::monitor::snapshot()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&netlane_core::planning::plan(
                &loaded.policies,
                &snapshot.adapters
            ))?
        );
        return Ok(());
    }
    if args.as_slice() == ["--snapshot"] {
        println!(
            "{}",
            serde_json::to_string_pretty(&netlane_windows::monitor::snapshot()?)?
        );
        return Ok(());
    }
    #[cfg(windows)]
    if args.len() == 4 && args[0] == "--default-connection-session" {
        let pid = args[2].parse()?;
        let start = args[3].parse()?;
        return tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(netlane_agent::default_connection::run(&args[1], pid, start));
    }
    #[cfg(windows)]
    if args.len() == 4 && args[0] == "--ui-session" {
        let pid = args[2].parse()?;
        let start = args[3].parse()?;
        return tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(netlane_agent::run_session(&args[1], pid, start));
    }
    anyhow::bail!(
        "Use --validate <JSON>, --plan <JSON> ou --snapshot. Sessões elevadas são iniciadas pelo painel."
    )
}
