//! Quality controls and synthetic demo. Native probes run only after manual Start.
use super::*;
use netlane_core::quality::{Mode, Outcome, Phase, ProbeResult, Rating, Target};

pub(super) struct Details {
    lines: Vec<String>,
}
impl Render for Details {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        div()
            .w(px(340.))
            .p(px(14.))
            .rounded(px(6.))
            .bg(t.elevated)
            .border_1()
            .border_color(t.border)
            .text_size(px(12.))
            .text_color(t.text)
            .flex()
            .flex_col()
            .gap(px(5.))
            .children(self.lines.iter().cloned().map(|line| div().child(line)))
    }
}
impl Workspace {
    fn quality_now(&self) -> Duration {
        self.started.elapsed() + Duration::from_secs(60)
    }
    pub(super) fn sync_quality(&mut self) {
        self.quality
            .sync(&self.snapshot.adapters, &self.selected, self.quality_now());
    }
    fn toggle_quality(&mut self, cx: &mut Context<Self>) {
        if self.quality.phase != Phase::Idle {
            self.quality.stop();
            self.quality_status = "Desativado · clique em Iniciar para medir novamente.".into();
            cx.notify();
            return;
        }
        if self.quality_dns_busy {
            self.quality_status = "Aguardando o fim da preparação anterior…".into();
            cx.notify();
            return;
        }
        if !self
            .snapshot
            .adapters
            .iter()
            .any(|a| self.selected.contains(&a.id) && a.connected && a.ipv4)
        {
            self.quality_status = "Selecione uma interface conectada com IPv4.".into();
            cx.notify();
            return;
        }
        let target = match Target::parse(
            self.quality_preferences.mode,
            self.quality_input.read(cx).text(),
        ) {
            Ok(t) => t,
            Err(e) => {
                self.quality_status = e.to_string();
                cx.notify();
                return;
            }
        };
        self.quality_preferences.target = target.label.clone();
        let generation = self.quality.begin(target.clone());
        if self.demo {
            let destination = target
                .host
                .parse()
                .unwrap_or_else(|_| "203.0.113.1".parse().unwrap());
            self.quality.activate(generation, destination);
            self.seed_quality_demo();
            self.quality_status =
                "Demonstração · amostras fictícias · nenhum tráfego enviado.".into();
            cx.notify();
            return;
        }
        self.quality_status = "Preparando destino IPv4…".into();
        self.quality_dns_busy = true;
        let cancel = self.quality.cancellation();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { netlane_windows::quality::resolve(&target, &cancel) })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.quality_dns_busy = false;
                match result {
                    Ok(destination) if ws.quality.activate(generation, destination) => {
                        ws.quality_status = format!(
                            "Ativo · {} → {destination} · intervalo ≥ 5 s · janela de 60 s",
                            ws.quality_preferences.target
                        );
                        if let Err(e) = ws
                            .quality_preferences
                            .save(&ws.profile.join("quality-settings.json"))
                        {
                            ws.quality_status
                                .push_str(&format!(" · destino não salvo: {e}"));
                        }
                        ws.refresh_quality(cx);
                    }
                    Err(e) if ws.quality.phase == Phase::Preparing => {
                        ws.quality.stop();
                        ws.quality_status = format!("Não foi possível preparar o destino: {e}");
                    }
                    _ => {}
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn seed_quality_demo(&mut self) {
        let end = self.quality_now();
        for n in 0..5 {
            let now = end - Duration::from_secs((4 - n) * 5);
            for request in self
                .quality
                .prepare(&self.snapshot.adapters, &self.selected, now)
            {
                let result = demo_result(&request.adapter, n, request.target.mode);
                self.quality.complete(request, result, now);
            }
        }
        self.sync_quality();
    }
    pub(super) fn refresh_quality(&mut self, cx: &mut Context<Self>) {
        if self.quality.phase != Phase::Active {
            return;
        }
        let now = self.quality_now();
        for request in self
            .quality
            .prepare(&self.snapshot.adapters, &self.selected, now)
        {
            if self.demo {
                let sample = self
                    .quality
                    .rows
                    .get(&request.adapter.id)
                    .map_or(0, |row| row.summary.samples as u64);
                let result = demo_result(&request.adapter, sample, request.target.mode);
                self.quality.complete(request, result, now);
            } else {
                cx.spawn(async move |this, cx| {
                    let (request, result) = cx
                        .background_executor()
                        .spawn(async move {
                            let result = netlane_windows::quality::probe(&request);
                            (request, result)
                        })
                        .await;
                    let _ = this.update(cx, |ws, cx| {
                        ws.sync_quality();
                        ws.quality.complete(request, result, ws.quality_now());
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        cx.notify();
    }
    fn quality_lines(&self, id: &str) -> Vec<String> {
        let mode = self.quality_preferences.mode;
        let ms = |v: Option<f64>| v.map_or("—".into(), |n| format!("{n:.0} ms"));
        let Some(row) = self.quality.rows.get(id) else {
            return vec![
                "Qualidade desativada".into(),
                "Clique em Iniciar para medir esta conexão.".into(),
            ];
        };
        let summary = &row.summary;
        let mut lines = vec![
            format!(
                "{}: {}",
                if mode == Mode::Icmp {
                    "Ping médio"
                } else {
                    "Tempo médio HTTPS"
                },
                ms(summary.latency)
            ),
            format!(
                "{}: {}",
                if mode == Mode::Icmp {
                    "Último ping"
                } else {
                    "Última resposta"
                },
                row.latency_label()
            ),
            format!(
                "{}: {}",
                if mode == Mode::Icmp {
                    "Perda de sondagens"
                } else {
                    "Falhas de conexão"
                },
                summary
                    .failure_percent
                    .map_or("—".into(), |p| format!("{p:.1}%"))
            ),
            format!("Jitter: {}", ms(summary.jitter)),
            format!("{} · {} amostras / 60 s", row.label(), summary.samples),
        ];
        if let Some(target) = &self.quality.target {
            lines.push(format!("Destino: {}", target.label));
        }
        if let Some(destination) = self.quality.destination {
            lines.push(format!("IPv4 comum: {destination}"));
        }
        if let Some((at, last)) = &row.last {
            lines.push(format!(
                "Atualizado há {} s · origem {} · índice {}",
                self.quality_now().saturating_sub(*at).as_secs(),
                last.source,
                last.index
            ));
            if matches!(last.outcome, Outcome::Error | Outcome::Unavailable) {
                lines.push(last.detail.clone());
            }
        }
        lines.push(
            if mode == Mode::Icmp {
                "ICMP pode ser filtrado; não confirma queda da Internet."
            } else {
                "Inclui TCP, TLS e cabeçalhos. Falhas não são perda de pacotes."
            }
            .into(),
        );
        if self.demo {
            lines.push("Demonstração · valores fictícios".into());
        }
        lines
    }
    pub(super) fn quality_indicator(
        &self,
        adapter: &netlane_core::Adapter,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = cx.theme();
        let row = self.quality.rows.get(&adapter.id);
        let rating = row
            .filter(|r| r.state.is_empty())
            .map_or(Rating::Unknown, |r| r.summary.rating);
        let color = match rating {
            Rating::Good => t.ok,
            Rating::Fair => t.warning,
            Rating::Poor | Rating::NoResponse => t.error,
            _ => t.text_muted,
        };
        let lines = self.quality_lines(&adapter.id);
        let label = row.map_or("Desativado".into(), |r| r.label());
        div()
            .id(gpui::SharedString::from(format!("quality-{}", adapter.id)))
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .tooltip(move |_, cx| {
                cx.new(|_| Details {
                    lines: lines.clone(),
                })
                .into()
            })
            .child(
                div()
                    .px(px(5.))
                    .rounded(px(4.))
                    .bg(color.opacity(0.12))
                    .text_color(color)
                    .text_size(px(11.))
                    .child(row.map_or("— ms".into(), |r| r.latency_label())),
            )
            .child(icon("lan", 20., color))
            .child(div().text_size(px(10.)).text_color(color).child(label))
    }
    pub(super) fn quality_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let active = self.quality.phase != Phase::Idle;
        let mode = self.quality_preferences.mode;
        let mut panel = div()
            .w_full()
            .min_w_0()
            .p(px(16.))
            .rounded(px(8.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Qualidade por conexão"),
                    )
                    .child(
                        self.button(
                            "quality-toggle",
                            if active {
                                "Parar"
                            } else if self.quality_dns_busy {
                                "Aguardando…"
                            } else {
                                "Iniciar"
                            },
                            cx,
                        )
                        .on_click(cx.listener(|ws, _, _, cx| ws.toggle_quality(cx))),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(t.text_muted)
                    .child(self.quality_status.clone()),
            );
        if !active {
            panel=panel.child(div().flex().flex_wrap().items_center().gap(px(8.))
                .child(self.button("quality-icmp",if mode==Mode::Icmp{"✓ Ping (ICMP)"}else{"Ping (ICMP)"},cx).on_click(cx.listener(|ws,_,_,cx|ws.change_quality_mode(Mode::Icmp,cx))))
                .child(self.button("quality-https",if mode==Mode::Https{"✓ Site (HTTPS)"}else{"Site (HTTPS)"},cx).on_click(cx.listener(|ws,_,_,cx|ws.change_quality_mode(Mode::Https,cx))))
                .child(div().w(px(270.)).min_w_0().p(px(8.)).rounded(px(4.)).bg(t.editor).border_1().border_color(t.border).child(self.quality_input.clone())))
                .child(div().text_size(px(11.)).text_color(t.text_muted).child("Mede o caminho até o destino. Inicia parado; resultados ficam apenas nesta sessão."));
        }
        #[cfg(feature = "gui-test")]
        if self.proof_quality_details
            && let Some(adapter) = self.snapshot.adapters.first()
        {
            let lines = self.quality_lines(&adapter.id);
            panel = panel.child(cx.new(|_| Details { lines }));
        }
        panel
    }
    fn change_quality_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        if self.quality.phase != Phase::Idle || self.quality_dns_busy {
            return;
        }
        self.quality_preferences.mode = mode;
        let text = self.quality_input.read(cx).text();
        if text == "1.1.1.1" || text == "https://example.com/" {
            self.quality_input.update(cx, |input, cx| {
                input.set_text(
                    if mode == Mode::Icmp {
                        "1.1.1.1"
                    } else {
                        "https://example.com/"
                    },
                    cx,
                )
            });
        }
        self.quality_status = "Desativado · escolha o destino e clique em Iniciar.".into();
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_quality_start(&mut self, cx: &mut Context<Self>) {
        assert!(self.demo && self.controller.is_none());
        self.toggle_quality(cx);
        assert_eq!(self.quality.phase, Phase::Active);
        assert!(self.quality.rows.values().all(|r| r.summary.samples == 5));
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_quality_stop(&mut self, cx: &mut Context<Self>) {
        assert!(self.demo);
        let cancellation = self.quality.cancellation();
        self.toggle_quality(cx);
        assert_eq!(self.quality.phase, Phase::Idle);
        assert!(cancellation.load(std::sync::atomic::Ordering::Relaxed));
        assert!(self.quality.rows.is_empty());
        self.proof_quality_details = false;
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_quality_details(&mut self, cx: &mut Context<Self>) {
        assert!(self.demo);
        self.proof_quality_details = true;
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_quality_https(&mut self, cx: &mut Context<Self>) {
        self.proof_quality_stop(cx);
        self.change_quality_mode(Mode::Https, cx);
        self.proof_quality_start(cx);
        assert!(
            self.quality_lines(&self.snapshot.adapters[0].id)
                .iter()
                .any(|line| line.starts_with("Tempo médio HTTPS"))
        );
        assert!(!self.profile.join("quality-settings.json").exists());
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_quality_active(&self) -> bool {
        self.quality.phase == Phase::Active
    }
}
fn demo_result(adapter: &netlane_core::Adapter, sample: u64, mode: Mode) -> ProbeResult {
    let missing = adapter.kind == "Wireless80211" && matches!(sample % 5, 1 | 3 | 4);
    let mut result = ProbeResult::new(
        if missing {
            Outcome::NoResponse
        } else {
            Outcome::Success
        },
        (!missing).then_some(if mode == Mode::Https {
            180. + sample as f64 * 3.
        } else if adapter.kind == "Wireless80211" {
            55. + sample as f64 * 2.
        } else {
            28. + (sample % 3) as f64
        }),
        "Amostra fictícia; nenhuma sondagem enviada.",
    );
    result.source = adapter.addresses.first().cloned().unwrap_or_default();
    result.index = adapter.luid as u32;
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn demo_results_are_explicit_fixtures_with_distinct_protocol_labels() {
        let snapshot = demo::snapshot();
        assert_eq!(
            demo_result(&snapshot.adapters[0], 0, Mode::Icmp).milliseconds,
            Some(28.)
        );
        assert_eq!(
            demo_result(&snapshot.adapters[1], 4, Mode::Icmp).outcome,
            Outcome::NoResponse
        );
        assert_eq!(
            demo_result(&snapshot.adapters[0], 0, Mode::Https).milliseconds,
            Some(180.)
        );
    }
}
