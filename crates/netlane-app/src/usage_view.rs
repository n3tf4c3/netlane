//! Consumo: data usage per network and per app, read from the history Windows keeps.
//! Reads run in the background while the page is open, at most once a minute.
use super::*;
use netlane_core::usage::{AppTotal, Period, Summary, format_bytes, summarize};

const REFRESH: Duration = Duration::from_secs(60);
/// Rows drawn before "Mostrar todos".
const TOP_APPS: usize = 25;
/// Hard ceiling on drawn rows; Windows can report hundreds of entries.
const MAX_APPS: usize = 200;

#[derive(Default)]
pub(super) struct UsageView {
    period: Period,
    summary: Option<Summary>,
    error: Option<String>,
    loading: bool,
    /// When the shown summary (or error) was read; reads repeat after `REFRESH`.
    read_at: Option<Instant>,
    show_all: bool,
}

impl Workspace {
    pub(crate) fn refresh_usage(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.page != Page::Usage || self.usage.loading {
            return;
        }
        if !force && self.usage.read_at.is_some_and(|at| at.elapsed() < REFRESH) {
            return;
        }
        let period = self.usage.period;
        if self.demo {
            self.usage.summary = Some(summarize(&demo::usage(period)));
            self.usage.error = None;
            self.usage.read_at = Some(Instant::now());
            cx.notify();
            return;
        }
        self.usage.loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { netlane_windows::usage::read(period) })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.usage.loading = false;
                if ws.usage.period != period {
                    // The period changed while reading: read the one now selected.
                    ws.refresh_usage(true, cx);
                    return;
                }
                ws.usage.read_at = Some(Instant::now());
                match result {
                    Ok(networks) => {
                        ws.usage.summary = Some(summarize(&networks));
                        ws.usage.error = None;
                    }
                    Err(e) => ws.usage.error = Some(format!("{e:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn select_usage_period(&mut self, period: Period, cx: &mut Context<Self>) {
        if self.usage.period == period {
            return;
        }
        self.usage.period = period;
        self.usage.summary = None;
        self.usage.error = None;
        self.usage.show_all = false;
        self.refresh_usage(true, cx);
        cx.notify();
    }
    /// Paths of the app rows on screen, for the icon cache.
    pub(super) fn usage_icon_paths(&self) -> Vec<String> {
        self.usage
            .summary
            .iter()
            .flat_map(|s| s.apps.iter().take(self.usage_rows()))
            .filter_map(|a| a.path.clone())
            .collect()
    }
    fn usage_rows(&self) -> usize {
        if self.usage.show_all {
            MAX_APPS
        } else {
            TOP_APPS
        }
    }
    /// Apps with a rule, matched like the rules match programs: by file name.
    fn usage_has_rule(&self, app: &AppTotal) -> bool {
        self.policies
            .iter()
            .any(|p| p.application_id.eq_ignore_ascii_case(&app.name))
    }

    pub(super) fn usage_page(&self, cx: &mut Context<Self>) -> gpui::Div {
        let t = cx.theme().clone();
        let compact = AppSettings::get(cx).compact;
        let button = |id: gpui::ElementId, label: &str, cx: &App| {
            self.button(id, label, cx)
                .when(compact, |b| b.px(px(8.)).py(px(3.)).text_size(px(12.)))
        };
        let mut toolbar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .child(div().text_color(t.text_muted).child("Período:"));
        for (i, period) in Period::ALL.into_iter().enumerate() {
            let active = self.usage.period == period;
            toolbar = toolbar.child(
                button(("usage-period", i).into(), period.label(), cx)
                    .when(active, |d| {
                        d.bg(t.selected).border_color(t.accent).cursor_default()
                    })
                    .when(!active, |d| {
                        d.on_click(cx.listener(move |ws, _, _, cx| {
                            ws.select_usage_period(period, cx);
                        }))
                    }),
            );
        }
        let status = if self.usage.loading {
            "Lendo os dados do Windows…".to_owned()
        } else if let Some(at) = self.usage.read_at {
            let seconds = at.elapsed().as_secs();
            if seconds < 60 {
                "Atualizado agora · nova leitura a cada minuto".to_owned()
            } else {
                format!("Atualizado há {} min", seconds / 60)
            }
        } else {
            String::new()
        };
        toolbar = toolbar
            .child(
                button("usage-refresh".into(), "Atualizar", cx).when(!self.usage.loading, |d| {
                    d.on_click(cx.listener(|ws, _, _, cx| ws.refresh_usage(true, cx)))
                }),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(t.text_muted)
                    .child(status),
            );
        let mut page = div()
            .flex()
            .flex_col()
            .gap(px(if compact { 10. } else { 16. }))
            .child(self.heading(
                "Consumo",
                "Dados por rede e por aplicativo registrados pelo próprio Windows (o mesmo de Configurações › Rede e Internet › Uso de dados), inclusive enquanto o NetLane esteve fechado.",
                cx,
            ))
            .child(toolbar);
        if let Some(error) = &self.usage.error {
            page = page.child(
                div()
                    .p(px(12.))
                    .rounded(px(4.))
                    .bg(t.warning.opacity(0.12))
                    .text_color(t.warning)
                    .child(format!("Não foi possível ler o consumo: {error}")),
            );
        }
        let Some(summary) = &self.usage.summary else {
            return page;
        };
        if summary.total.total() == 0 {
            return page.child(
                div()
                    .p(px(24.))
                    .text_color(t.text_muted)
                    .child("O Windows não registrou tráfego neste período."),
            );
        }
        let mut tiles = div()
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .child(self.stat_tile(format_bytes(summary.total.total()), "Total", cx))
            .child(self.stat_tile(
                format_bytes(summary.total.received),
                "Recebido (download)",
                cx,
            ))
            .child(self.stat_tile(format_bytes(summary.total.sent), "Enviado (upload)", cx));
        for (kind, bytes) in &summary.kinds {
            tiles = tiles.child(self.stat_tile(format_bytes(bytes.total()), kind.label(), cx));
        }
        page = page.child(tiles);

        // Networks: one row per Windows profile (each Wi-Fi SSID is its own network).
        let largest = summary
            .networks
            .first()
            .map_or(1, |n| n.bytes.total().max(1));
        let mut networks = self.usage_card("Redes", cx);
        for network in &summary.networks {
            networks = networks.child(self.usage_row(
                None,
                network.name.clone(),
                network.kind.label().to_owned(),
                network.bytes.total(),
                largest,
                format!(
                    "↓ {} · ↑ {}",
                    format_bytes(network.bytes.received),
                    format_bytes(network.bytes.sent)
                ),
                false,
                cx,
            ));
        }
        page = page.child(networks);

        let largest = summary.apps.first().map_or(1, |a| a.bytes.total().max(1));
        let shown = summary.apps.len().min(self.usage_rows());
        let mut apps = self.usage_card("Aplicativos", cx);
        for app in summary.apps.iter().take(shown) {
            let split = app
                .by_kind
                .iter()
                .map(|(kind, bytes)| format!("{} {}", kind.label(), format_bytes(*bytes)))
                .collect::<Vec<_>>()
                .join(" · ");
            apps = apps.child(self.usage_row(
                Some(app.path.as_deref()),
                app.name.clone(),
                app.detail.clone(),
                app.bytes.total(),
                largest,
                format!(
                    "↓ {} · ↑ {}{}",
                    format_bytes(app.bytes.received),
                    format_bytes(app.bytes.sent),
                    // The split matters only when the period used more than one type.
                    if summary.kinds.len() > 1 {
                        format!(" · {split}")
                    } else {
                        String::new()
                    }
                ),
                self.usage_has_rule(app),
                cx,
            ));
        }
        let hidden = summary.apps.len() - shown;
        if hidden > 0 && !self.usage.show_all {
            apps = apps.child(
                button(
                    "usage-all".into(),
                    &format!("Mostrar todos (mais {hidden})"),
                    cx,
                )
                .on_click(cx.listener(|ws, _, _, cx| {
                    ws.usage.show_all = true;
                    cx.notify();
                })),
            );
        } else if hidden > 0 {
            apps = apps.child(
                div()
                    .text_size(px(12.))
                    .text_color(t.text_muted)
                    .child(format!("E mais {hidden} com consumo menor.")),
            );
        }
        page.child(apps)
    }

    fn usage_card(&self, title: &str, cx: &App) -> gpui::Div {
        let t = cx.theme();
        let compact = AppSettings::get(cx).compact;
        div()
            .p(px(if compact { 12. } else { 18. }))
            .bg(t.panel)
            .rounded(px(8.))
            .border_1()
            .border_color(t.border)
            .flex()
            .flex_col()
            .gap(px(if compact { 6. } else { 10. }))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.text_strong)
                    .child(title.to_owned()),
            )
    }

    /// `icon`: `Some(path)` draws the program icon (or the generic one).
    #[allow(clippy::too_many_arguments)]
    fn usage_row(
        &self,
        icon_path: Option<Option<&str>>,
        name: String,
        detail: String,
        total: u64,
        largest: u64,
        traffic: String,
        has_rule: bool,
        cx: &App,
    ) -> gpui::Div {
        let t = cx.theme();
        let share = (total as f64 / largest as f64).clamp(0., 1.) as f32;
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .when_some(icon_path, |d, path| {
                d.child(self.application_icon(path, t.text_muted))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().text_color(t.text_strong).truncate().child(name))
                            .when(has_rule, |d| {
                                d.child(
                                    div()
                                        .px(px(6.))
                                        .rounded(px(3.))
                                        .bg(t.accent.opacity(0.15))
                                        .text_color(t.accent)
                                        .text_size(px(11.))
                                        .child("com regra"),
                                )
                            })
                            .child(div().flex_1())
                            .child(
                                div()
                                    .flex_none()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.text_strong)
                                    .child(format_bytes(total)),
                            ),
                    )
                    .child(
                        div().h(px(6.)).w_full().rounded(px(3.)).bg(t.border).child(
                            div()
                                .h_full()
                                .w(gpui::relative(share.max(0.005)))
                                .rounded(px(3.))
                                .bg(t.accent.opacity(0.85)),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .text_size(px(11.))
                            .text_color(t.text_muted)
                            .child(div().flex_1().min_w_0().truncate().child(detail))
                            .child(div().flex_none().child(traffic)),
                    ),
            )
    }
}
#[cfg(feature = "gui-test")]
impl Workspace {
    pub(crate) fn proof_usage_ready(&self) -> bool {
        self.usage
            .summary
            .as_ref()
            .is_some_and(|s| s.apps.len() > 3 && s.kinds.len() == 2)
    }
}
