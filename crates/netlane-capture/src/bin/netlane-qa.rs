//! Standalone offline QA helper. Deliberately depends on no Windows or AI adapter.
use anyhow::{Result, bail};
use netlane_capture::qa::{self, FixtureOptions, Stress};
use std::path::PathBuf;
fn run() -> Result<bool> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        bail!(
            "Use --generate-fixtures PASTA ou --verify-fixtures PASTA --output RESULTADOS. Ambos usam somente dados sintéticos."
        );
    };
    if command == "--help" {
        println!(
            "NetLane QA offline\n--generate-fixtures PASTA_NOVA [--records 70000] [--packet-bytes 2112]\n--verify-fixtures PASTA --output PASTA_NOVA [--stress-seconds 2] [--stress-mib 16] [--stress-pps 20000]\nCaminhos absolutos locais. Sem Npcap, rede ou inferência."
        );
        return Ok(true);
    }
    let root = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("Pasta obrigatória."))?,
    );
    let mut fixtures = FixtureOptions::default();
    let mut stress = Stress::default();
    let mut output = None;
    let mut seen = std::collections::BTreeSet::new();
    while let Some(option) = args.next() {
        if !seen.insert(option.clone()) {
            bail!("Opção repetida: {option}");
        }
        let value = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("Valor obrigatório para {option}"))?;
        match (command.as_str(), option.as_str()) {
            ("--generate-fixtures", "--records") => fixtures.records = value.parse()?,
            ("--generate-fixtures", "--packet-bytes") => fixtures.packet_bytes = value.parse()?,
            ("--verify-fixtures", "--output") => output = Some(PathBuf::from(value)),
            ("--verify-fixtures", "--stress-seconds") => stress.seconds = value.parse()?,
            ("--verify-fixtures", "--stress-mib") => stress.limit_mib = value.parse()?,
            ("--verify-fixtures", "--stress-pps") => stress.packets_per_second = value.parse()?,
            _ => bail!("Opção inválida: {option}"),
        }
    }
    match command.as_str() {
        "--generate-fixtures" => {
            let manifest = qa::generate(&root, fixtures)?;
            println!(
                "{} fixtures sintéticas geradas em {}",
                manifest.files.len(),
                root.display()
            );
            Ok(true)
        }
        "--verify-fixtures" => {
            let output = output.ok_or_else(|| anyhow::anyhow!("--output obrigatório."))?;
            let result = qa::verify(&root, &output, stress)?;
            for check in &result.checks {
                println!(
                    "{} {} ({} ms){}",
                    if check.passed { "PASS" } else { "FAIL" },
                    check.name,
                    check.elapsed_ms,
                    check
                        .error
                        .as_ref()
                        .map(|e| format!(": {e}"))
                        .unwrap_or_default()
                );
            }
            println!("Recibo: {}", output.join("verification.json").display());
            Ok(result.passed)
        }
        _ => bail!("Comando inválido: {command}"),
    }
}
fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("QA falhou: {e:#}");
            std::process::exit(1);
        }
    }
}
