//! Local drafts: immutable last-turn evidence, no capture payload or authentication.
use crate::Context;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    pub provider: String,
    pub model: String,
    pub status: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Draft {
    pub context: Context,
    pub problem: String,
    pub question: String,
    pub response: Option<Response>,
}
#[derive(Debug, Serialize)]
pub struct FileHash {
    pub file: String,
    pub bytes: usize,
    pub sha256: String,
}
#[derive(Debug, Serialize)]
pub struct Receipt {
    pub schema: u32,
    pub investigation_id: String,
    pub source_sha256: String,
    pub packet_numbers: Vec<u64>,
    pub ai_included: bool,
    pub references_checked: bool,
    pub files: Vec<FileHash>,
}
impl Draft {
    pub fn validate(&self) -> Result<()> {
        self.context.prompt(&self.problem, &self.question)?;
        if let Some(r) = &self.response {
            for v in [&r.provider, &r.model, &r.status] {
                ensure!(
                    !v.trim().is_empty() && v.len() <= 256 && !v.chars().any(char::is_control),
                    "Metadado da resposta inválido."
                );
            }
            ensure!(!r.text.trim().is_empty(), "Resposta vazia.");
            self.context.validate_references(&r.text)?;
        }
        Ok(())
    }
    pub fn markdown(&self) -> Result<String> {
        self.validate()?;
        let mut out = "# NetLane — rascunho de diagnóstico de rede\n\nRascunho técnico para revisão humana. As referências foram conferidas apenas contra o contexto selecionado; isso não valida as conclusões nem comprova um problema ou invasão.\n\n".to_string();
        section(
            &mut out,
            "Problema relatado",
            if self.problem.trim().is_empty() {
                "Não informado pelo usuário."
            } else {
                &self.problem
            },
        );
        section(&mut out, "Pergunta e escopo", &self.question);
        section(
            &mut out,
            "Origem e integridade",
            &format!(
                "Investigação: {}\nFormato: {}\nSHA-256 do original: {}\nPacotes no arquivo: {}\nPacotes analisados: {}\nPacotes fora da análise: {}\nDetalhes selecionados: {}\nPeríodo dos detalhes: {} a {} (UTC, nanossegundos Unix; não necessariamente o período inteiro).",
                self.context.investigation_id,
                self.context.format,
                self.context.sha256,
                self.context.total_packets,
                self.context.inspected_packets,
                self.context.omitted_packets,
                self.context.packets.len(),
                self.context
                    .packets
                    .iter()
                    .filter_map(|p| p.unix_nanos.as_ref().and_then(|n| n.parse::<i128>().ok()))
                    .min()
                    .map_or("ausente".into(), |n| n.to_string()),
                self.context
                    .packets
                    .iter()
                    .filter_map(|p| p.unix_nanos.as_ref().and_then(|n| n.parse::<i128>().ok()))
                    .max()
                    .map_or("ausente".into(), |n| n.to_string())
            ),
        );
        if let Some(a) = &self.context.analysis {
            section(&mut out, "Agregados e comparação local", &a.summary);
            section(
                &mut out,
                "Referência da comparação",
                a.baseline_sha256
                    .as_deref()
                    .unwrap_or("Nenhuma referência selecionada."),
            );
        }
        for p in &self.context.packets {
            section(
                &mut out,
                &format!("Evidência do pacote {}", p.number),
                &format!(
                    "[[packet:{}:{}]]\n{} · {} → {}\nInterface: {} · seção {} · link type {}\nCaplen: {} bytes · tamanho original: {} bytes\nObservação local: {}",
                    self.context.sha256,
                    p.number,
                    p.protocol,
                    p.source,
                    p.destination,
                    p.interface,
                    p.section,
                    p.link_type,
                    p.captured_bytes,
                    p.original_bytes,
                    p.observation
                ),
            );
        }
        if let Some(r) = &self.response {
            section(
                &mut out,
                "Resposta assistida — requer revisão",
                &format!(
                    "Provedor: {}\nModelo no envio: {}\nEstado do turno: {}\n\n{}",
                    r.provider, r.model, r.status, r.text
                ),
            );
        } else {
            out.push_str("## Hipóteses e recomendações\n\nSem resposta de IA incluída. Nenhum diagnóstico automático foi emitido. Registre hipóteses e próximos passos após conferir as evidências.\n\n");
        }
        section(
            &mut out,
            "Limitações e método",
            "Contexto estruturado de uma captura importada ou segmento fechado; a origem da coleta, quando disponível, consta no resumo local. Agregados cobrem somente os registros declarados como analisados; detalhes são uma seleção das amostras. Endereços e portas são dados sensíveis. Sem payload, remontagem, DNS/TLS decodificado ou correlação com processo. Serviço por porta e desvios de baseline são heurísticas. A comparação pode usar filtro, período e ponto de coleta diferentes. Não anexa o PCAP original, caminhos dos arquivos ou credenciais do perfil. Textos do usuário e provedor podem conter informações sensíveis e exigem revisão. A resposta, se incluída, é somente do último turno, com o contexto desse envio; a conversa anterior não está anexada. Referências ausentes/fora do contexto impedem exportar a resposta; ausência de citações não prova correção. Nenhum comando ou próximo passo foi executado pelo relatório.",
        );
        // Keep user/provider data in literal indented code blocks, never executable Markdown.
        Ok(out)
    }
    pub fn html(&self) -> Result<String> {
        let markdown = self.markdown()?;
        let mut body = String::new();
        let mut literal = String::new();
        for line in markdown.lines() {
            // Only fixed, unindented headings become markup. Inserted data is always indented.
            let heading = line
                .strip_prefix("## ")
                .map(|t| (2, t))
                .or_else(|| line.strip_prefix("# ").map(|t| (1, t)));
            if let Some((level, title)) = heading {
                if !literal.trim().is_empty() {
                    body.push_str(&format!("<pre>{}</pre>", escape(&literal)));
                }
                literal.clear();
                body.push_str(&format!("<h{level}>{}</h{level}>", escape(title)));
            } else {
                literal.push_str(line.strip_prefix("    ").unwrap_or(line));
                literal.push('\n');
            }
        }
        if !literal.trim().is_empty() {
            body.push_str(&format!("<pre>{}</pre>", escape(&literal)));
        }
        Ok(format!(
            "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\"><title>NetLane — rascunho de diagnóstico</title><style>body{{max-width:1000px;margin:32px auto;padding:0 20px;background:#f4f6f8;color:#172b39;font:15px/1.6 Segoe UI,sans-serif}}h1{{font-size:26px;color:#007d72}}h2{{font-size:18px;margin-top:28px}}pre{{font:inherit;white-space:pre-wrap;overflow-wrap:anywhere;background:white;border:1px solid #d8e1e5;border-radius:8px;padding:16px}}@media print{{body{{margin:0;background:white}}}}</style></head><body>{body}</body></html>"
        ))
    }
    pub fn save(&self, directory: &Path) -> Result<Receipt> {
        ensure!(directory.is_absolute(), "Destino deve ser absoluto.");
        let files = [
            ("report.md", self.markdown()?.into_bytes()),
            ("report.html", self.html()?.into_bytes()),
            ("context.json", serde_json::to_vec_pretty(self)?),
        ];
        let receipt = Receipt {
            schema: 1,
            investigation_id: self.context.investigation_id.clone(),
            source_sha256: self.context.sha256.clone(),
            packet_numbers: self.context.packets.iter().map(|p| p.number).collect(),
            ai_included: self.response.is_some(),
            references_checked: self.response.is_some(),
            files: files
                .iter()
                .map(|(name, data)| FileHash {
                    file: (*name).into(),
                    bytes: data.len(),
                    sha256: format!("{:x}", Sha256::digest(data)),
                })
                .collect(),
        };
        fs::create_dir(directory)?;
        for (name, data) in &files {
            publish(directory, name, data)?;
            ensure!(
                fs::read(directory.join(name))? == *data,
                "Arquivo salvo diverge do rascunho; recibo não publicado."
            );
        }
        publish(
            directory,
            "receipt.json",
            &serde_json::to_vec_pretty(&receipt)?,
        )?;
        Ok(receipt)
    }
}
fn section(out: &mut String, title: &str, text: &str) {
    out.push_str(&format!("## {title}\n\n"));
    for line in text.lines() {
        out.push_str("    ");
        // Replace bidi/control characters that could conceal content. Keep tabs printable.
        out.extend(line.chars().map(|c| {
            if c.is_control() || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}') {
                '�'
            } else {
                c
            }
        }));
        out.push('\n');
    }
    out.push('\n');
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn publish(directory: &Path, name: &str, data: &[u8]) -> Result<()> {
    let part = directory.join(format!("{name}.part"));
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&part)?;
    f.write_all(data)?;
    f.sync_all()?;
    drop(f);
    fs::rename(part, directory.join(name))?;
    Ok(())
}
