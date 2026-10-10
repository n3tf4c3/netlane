fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    anyhow::ensure!(
        args.len() == 2 || (args.len() == 3 && args[2] == "--cancel"),
        "probe <codex.exe> <absolute-netlane-profile> [--cancel]"
    );
    let job = netlane_ai::codex::Probe::start(args[0].clone().into(), args[1].clone().into())?;
    let cancel = args.len() == 3;
    if cancel {
        job.cancel();
    }
    while !job.state().done {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let result = job.state();
    println!(
        "{}\nConnected: {}\nOwnProfileAuthenticated: {}\nCatalogModels: {}\nInferenceSent: false",
        result.detail,
        result.connected,
        result.authenticated,
        result.models.len()
    );
    if cancel {
        anyhow::ensure!(
            !result.connected && result.detail.contains("Verificação cancelada."),
            "Cancellation was not confirmed"
        );
    } else {
        anyhow::ensure!(result.connected, "Metadata verification failed");
    }
    Ok(())
}
