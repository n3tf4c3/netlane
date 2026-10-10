fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    anyhow::ensure!(
        args.len() == 2,
        "probe_bridge <codex.exe> <absolute-fresh-profile>"
    );
    let profile = std::path::PathBuf::from(&args[1]);
    anyhow::ensure!(!profile.exists(), "Escolha perfil de prova novo.");
    netlane_ai::bridge::verify_profile(
        &netlane_ai::bridge::CodexInfo {
            executable: args[0].clone().into(),
            version: netlane_ai::bridge::CODEX_VERSION.into(),
        },
        profile.join("home"),
        profile.join("room"),
    )?;
    println!(
        "ProposalProfileVerified: true\nEphemeralThread: true\nChildExitConfirmed: true\nPersonalProfileUsed: false\nInferenceSent: false\nLoginStarted: false"
    );
    Ok(())
}
