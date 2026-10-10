//! One opt-in, hidden process for a bounded metadata probe. No inference or login.
use crate::protocol::Rpc;
use anyhow::{Context as _, Result, ensure};
use serde_json::{Value, json};
use std::{
    io::Read as _,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, BufReader},
    process::Command,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}
#[derive(Clone, Debug, Default)]
pub struct ProbeState {
    pub done: bool,
    pub connected: bool,
    pub authenticated: bool,
    pub models: Vec<Model>,
    pub detail: String,
}
pub struct Probe {
    state: Arc<Mutex<ProbeState>>,
    cancel: Arc<AtomicBool>,
}
impl Probe {
    pub fn start(executable: PathBuf, profile: PathBuf) -> Result<Self> {
        validate_executable(&executable)?;
        ensure!(profile.is_absolute(), "Perfil NetLane deve ser absoluto.");
        let state = Arc::new(Mutex::new(ProbeState {
            detail: "Verificando conexão com Codex…".into(),
            ..ProbeState::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let shared = state.clone();
        let stopped = cancel.clone();
        std::thread::Builder::new()
            .name("netlane-codex-probe".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(anyhow::Error::from)
                    .and_then(|runtime| runtime.block_on(probe(&executable, &profile, &stopped)));
                let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
                match result {
                    Ok(result) => *state = result,
                    Err(error) => {
                        state.detail = format!("Não foi possível verificar: {error}");
                    }
                }
                state.done = true;
            })?;
        Ok(Self { state, cancel })
    }
    pub fn state(&self) -> ProbeState {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.cancel();
    }
}
pub fn validate_executable(path: &Path) -> Result<()> {
    let text = path.to_string_lossy();
    ensure!(
        path.is_absolute()
            && path.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe")),
        "Escolha o executável nativo codex.exe instalado."
    );
    ensure!(
        !text.starts_with("\\\\")
            && !text.starts_with("//")
            && !text.get(2..).is_some_and(|s| s.contains(':')),
        "Executável remoto ou dispositivo recusado."
    );
    Ok(())
}
/// Bounded discovery. Never runs a shim, Node, shell, installer or package manager.
pub fn discover() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    discover_in(std::env::split_paths(&path).take(64))
}
pub fn discover_in(directories: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    let triple = if cfg!(target_arch = "aarch64") {
        "aarch64-pc-windows-msvc"
    } else {
        "x86_64-pc-windows-msvc"
    };
    let platform = if cfg!(target_arch = "aarch64") {
        "codex-win32-arm64"
    } else {
        "codex-win32-x64"
    };
    for directory in directories.into_iter().take(64) {
        let direct = directory.join("codex.exe");
        if validate_executable(&direct).is_ok() {
            return Some(direct);
        }
        let root = directory.join("node_modules/@openai/codex");
        let package = root.join("package.json");
        if !package
            .metadata()
            .is_ok_and(|m| m.is_file() && m.len() <= 16 * 1024)
        {
            continue;
        }
        let Ok(file) = std::fs::File::open(&package) else {
            continue;
        };
        let mut data = Vec::new();
        if file.take(16 * 1024 + 1).read_to_end(&mut data).is_err() || data.len() > 16 * 1024 {
            continue;
        }
        let Ok(json) = serde_json::from_slice::<Value>(&data) else {
            continue;
        };
        if json.get("name").and_then(Value::as_str) != Some("@openai/codex") {
            continue;
        }
        for vendor in [
            root.join(format!("node_modules/@openai/{platform}/vendor")),
            directory.join(format!("node_modules/@openai/{platform}/vendor")),
            root.join("vendor"),
        ] {
            for folder in ["bin", "codex"] {
                let native = vendor.join(triple).join(folder).join("codex.exe");
                if validate_executable(&native).is_ok() {
                    return Some(native);
                }
            }
        }
    }
    None
}
pub fn launch_arguments() -> Vec<String> {
    let mut args = vec!["--strict-config".into()];
    for feature in [
        "shell_tool",
        "unified_exec",
        "hooks",
        "apps",
        "multi_agent",
        "browser_use",
        "computer_use",
        "view_image",
        "code_mode_host",
        "daemon_auto_start",
    ] {
        args.extend(["--disable".into(), feature.into()]);
    }
    for value in [
        "project_doc_max_bytes=0",
        "features.skip_host_skill_discovery=true",
        "history.persistence=\"none\"",
        "web_search=\"disabled\"",
    ] {
        args.extend(["-c".into(), value.into()]);
    }
    args.extend(["app-server".into(), "--listen".into(), "stdio://".into()]);
    args
}
async fn probe(executable: &Path, profile: &Path, cancel: &AtomicBool) -> Result<ProbeState> {
    validate_executable(executable)?;
    let home = profile.join("codex-bridge/codex-home");
    let room = profile.join("codex-bridge/empty-room");
    std::fs::create_dir_all(&home)?;
    std::fs::create_dir_all(&room)?;
    let mut command = Command::new(executable);
    command
        .args(launch_arguments())
        .current_dir(&room)
        .env("CODEX_HOME", &home)
        .env_remove("OPENAI_API_KEY")
        .env_remove("CODEX_API_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().context("Codex não iniciou")?;
    let stderr = child.stderr.take().context("Codex sem stderr")?;
    let drain = tokio::spawn(async move {
        let mut stream = stderr;
        let mut bytes = [0u8; 4096];
        while stream.read(&mut bytes).await.is_ok_and(|n| n > 0) {}
    });
    let mut rpc = Rpc::new(
        BufReader::new(child.stdout.take().context("Codex sem stdout")?),
        child.stdin.take().context("Codex sem stdin")?,
    );
    let result = tokio::select! {
        result=tokio::time::timeout(Duration::from_secs(30),metadata(&mut rpc))=>result.unwrap_or_else(|_|Err(anyhow::anyhow!("Verificação Codex excedeu 30 segundos."))),
        _=async {while !cancel.load(Ordering::Relaxed) {tokio::time::sleep(Duration::from_millis(50)).await;}}=>Err(anyhow::anyhow!("Verificação cancelada.")),
    };
    let kill = child.start_kill();
    let exit = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    drain.abort();
    kill.context("Não foi possível encerrar o processo da verificação")?;
    exit.context("Processo Codex não confirmou saída em 5 segundos")??;
    result
}
async fn metadata<R: tokio::io::AsyncBufRead + Unpin, W: tokio::io::AsyncWrite + Unpin>(
    rpc: &mut Rpc<R, W>,
) -> Result<ProbeState> {
    rpc.call("initialize",json!({"clientInfo":{"name":"netlane_diagnostics","title":"NetLane","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":false}})).await?;
    rpc.initialized().await?;
    let config = rpc
        .call("config/read", json!({"includeLayers":false}))
        .await?;
    let features = config
        .get("config")
        .and_then(|c| c.get("features"))
        .context("Codex não confirmou a configuração da ponte")?;
    for key in [
        "shell_tool",
        "unified_exec",
        "hooks",
        "apps",
        "multi_agent",
        "browser_use",
        "computer_use",
        "view_image",
        "code_mode_host",
        "daemon_auto_start",
    ] {
        ensure!(
            features.get(key).and_then(Value::as_bool) == Some(false),
            "Codex não confirmou a restrição de {key}."
        );
    }
    ensure!(
        features
            .get("skip_host_skill_discovery")
            .and_then(Value::as_bool)
            == Some(true),
        "Codex não confirmou o isolamento das skills pessoais."
    );
    let servers = rpc
        .call("mcpServerStatus/list", json!({"limit":100}))
        .await?;
    ensure!(
        servers
            .get("data")
            .and_then(Value::as_array)
            .is_some_and(|s| s.is_empty())
            && servers.get("nextCursor").is_none_or(Value::is_null),
        "O perfil da ponte contém servidores MCP."
    );
    let account = rpc
        .call("account/read", json!({"refreshToken":false}))
        .await?;
    let authenticated = account.get("account").is_some_and(|a| !a.is_null());
    let mut models = vec![];
    let mut cursor = Value::Null;
    for _ in 0..5 {
        let catalog = rpc
            .call(
                "model/list",
                json!({"limit":20,"includeHidden":false,"cursor":cursor}),
            )
            .await?;
        let entries = catalog
            .get("data")
            .and_then(Value::as_array)
            .context("Catálogo Codex inválido")?;
        ensure!(
            entries.len() <= 100 && models.len() + entries.len() <= 100,
            "Catálogo Codex excede o limite."
        );
        for entry in entries {
            let id = entry
                .get("model")
                .or_else(|| entry.get("id"))
                .and_then(Value::as_str)
                .context("Modelo Codex sem identificador")?;
            let name = entry
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or(id);
            ensure!(
                id.len() <= 128 && name.len() <= 256,
                "Nome de modelo Codex excede o limite."
            );
            models.push(Model {
                id: id.into(),
                name: name.into(),
                is_default: entry.get("isDefault").and_then(Value::as_bool) == Some(true),
            });
        }
        cursor = catalog.get("nextCursor").cloned().unwrap_or(Value::Null);
        if cursor.is_null() {
            break;
        }
        ensure!(
            cursor.as_str().is_some_and(|s| s.len() <= 1024),
            "Cursor Codex inválido."
        );
    }
    ensure!(cursor.is_null(), "Catálogo Codex excede cinco páginas.");
    Ok(ProbeState {
        done: true,
        connected: true,
        authenticated,
        models,
        detail: if authenticated {
            "Ponte verificada. Conta presente no perfil próprio do NetLane."
        } else {
            "Ponte verificada. O perfil próprio do NetLane ainda não tem uma conta conectada."
        }
        .into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::JsonLines;
    use tokio::io::{AsyncWriteExt as _, duplex, split};

    fn config() -> Value {
        json!({"config":{"features":{
            "shell_tool":false,"unified_exec":false,"hooks":false,"apps":false,
            "multi_agent":false,"browser_use":false,"computer_use":false,
            "view_image":false,"code_mode_host":false,"daemon_auto_start":false,
            "skip_host_skill_discovery":true
        }}})
    }
    async fn exchange(replies: Vec<(&'static str, Value)>) -> Result<ProbeState> {
        let (client, server) = duplex(16 * 1024);
        let (reader, writer) = split(client);
        let (server_reader, mut server_writer) = split(server);
        let worker = tokio::spawn(async move {
            let mut lines = JsonLines::new(BufReader::new(server_reader));
            for (method, result) in replies {
                let mut request = lines.next().await.unwrap();
                if request["method"] == "initialized" {
                    assert!(request.get("id").is_none());
                    request = lines.next().await.unwrap();
                }
                assert_eq!(request["method"], method);
                if method == "account/read" {
                    assert_eq!(request["params"]["refreshToken"], false);
                }
                let response = json!({"id":request["id"],"result":result});
                let mut bytes = serde_json::to_vec(&response).unwrap();
                bytes.push(b'\n');
                server_writer.write_all(&bytes).await.unwrap();
            }
        });
        let mut rpc = Rpc::new(BufReader::new(reader), writer);
        let result = metadata(&mut rpc).await;
        worker.await.unwrap();
        result
    }

    #[tokio::test]
    async fn metadata_handshake_lists_pages_without_retaining_account_identity() {
        let state = exchange(vec![
            ("initialize", json!({})),
            ("config/read", config()),
            ("mcpServerStatus/list", json!({"data":[],"nextCursor":null})),
            ("account/read", json!({"account":{"type":"chatgpt","email":"fixture@example.invalid"}})),
            ("model/list", json!({"data":[{"model":"fixture-1","displayName":"Fixture","isDefault":true}],"nextCursor":"page-2"})),
            ("model/list", json!({"data":[{"id":"fixture-2"}],"nextCursor":null})),
        ]).await.unwrap();
        assert!(state.connected && state.done && state.authenticated);
        assert_eq!(state.models.len(), 2);
        assert!(state.models[0].is_default);
        assert_eq!(state.models[1].name, "fixture-2");
        assert!(!format!("{state:?}").contains("fixture@example.invalid"));
    }

    #[tokio::test]
    async fn incompatible_tool_or_skill_configuration_stops_before_account_query() {
        for key in ["shell_tool", "skip_host_skill_discovery"] {
            let mut settings = config();
            settings["config"]["features"][key] = json!(key == "shell_tool");
            let error = exchange(vec![("initialize", json!({})), ("config/read", settings)])
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains(if key == "shell_tool" { key } else { "skills" }));
        }
    }

    #[tokio::test]
    async fn mcp_server_presence_or_omitted_page_is_refused() {
        for servers in [
            json!({"data":[{"name":"fixture-personal-server"}],"nextCursor":null}),
            json!({"data":[],"nextCursor":"another-page"}),
        ] {
            let error = exchange(vec![
                ("initialize", json!({})),
                ("config/read", config()),
                ("mcpServerStatus/list", servers),
            ])
            .await
            .unwrap_err()
            .to_string();
            assert!(error.contains("MCP"));
        }
    }
}
