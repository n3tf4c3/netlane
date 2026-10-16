use super::*;
use crate::scrollbar::ScrollbarExt;
use crate::ui::{Tip, icon, icon_button};

/// Reasoning effort names reported by the Codex catalog, in Portuguese.
fn effort_label(effort: &str) -> &str {
    match effort {
        "low" => "Baixo",
        "medium" => "Médio",
        "high" => "Alto",
        "xhigh" => "Muito alto",
        "max" => "Máximo",
        other => other,
    }
}

impl CodexView {
    pub(super) fn chip(
        id: impl Into<gpui::ElementId>,
        label: &str,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let t = cx.theme();
        div()
            .id(id)
            .h(px(24.))
            .px(px(8.))
            .flex()
            .flex_none()
            .items_center()
            .rounded(px(4.))
            .border_1()
            .border_color(t.border)
            .bg(t.elevated)
            .text_size(px(11.5))
            .cursor_pointer()
            .hover(|d| d.bg(t.hover))
            .child(label.to_owned())
    }
    fn field(input: Entity<TextInput>, cx: &App) -> gpui::Div {
        let t = cx.theme();
        div()
            .min_h(px(32.))
            .px(px(8.))
            .flex()
            .items_center()
            .rounded(px(4.))
            .border_1()
            .border_color(t.border)
            .bg(t.editor)
            .overflow_hidden()
            .child(input)
    }
    /// One header row: icon, muted label, value and optional trailing element.
    fn bar(cx: &App) -> gpui::Div {
        let t = cx.theme();
        div()
            .min_h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(10.))
            .border_b_1()
            .border_color(t.divider)
            .text_size(px(11.5))
    }
    /// Connection, notebook, bundle and report actions, opened from the header.
    fn render_options(&self, cx: &mut Context<Self>) -> gpui::Div {
        let t = cx.theme().clone();
        let busy = self.probe.as_ref().is_some_and(|p| !p.state().done);
        let row = || div().flex().flex_wrap().gap(px(4.));
        div()
            .flex_none()
            .px(px(10.))
            .py(px(8.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .border_b_1()
            .border_color(t.divider)
            .text_size(px(11.5))
            .child(
                row()
                    .child(
                        Self::chip(
                            "codex-check",
                            if self.demo {
                                "Simular verificação"
                            } else {
                                "Verificar conexão"
                            },
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.check_connection(cx))),
                    )
                    .when(busy, |d| {
                        d.child(
                            Self::chip("codex-cancel-probe", "Cancelar verificação", cx).on_click(
                                cx.listener(|this, _, _, cx| {
                                    if let Some(probe) = &this.probe {
                                        probe.cancel();
                                    }
                                    cx.notify();
                                }),
                            ),
                        )
                    })
                    .when(self.agent.is_some() && !self.agent_ready, |d| {
                        d.child(
                            Self::chip("codex-login-cancel", "Cancelar login", cx).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.agent_command(Command::CancelLogin, cx)
                                }),
                            ),
                        )
                    })
                    .when(!self.demo, |d| {
                        d.child(Self::chip("codex-exe", "Caminho do Codex", cx).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.edit_executable = !this.edit_executable;
                                cx.notify();
                            }),
                        ))
                    }),
            )
            .when(self.edit_executable, |d| {
                d.child(Self::field(self.executable.clone(), cx))
            })
            .when(!self.probe_state.detail.is_empty(), |d| {
                d.child(
                    div()
                        .text_color(t.text_muted)
                        .child(self.probe_state.detail.clone()),
                )
            })
            .child(
                row()
                    .child(
                        Self::chip("notebook-open", "Abrir caderno", cx).on_click(cx.listener(
                            |this, _, window, cx| this.open_notebook(window, cx),
                        )),
                    )
                    .child(
                        Self::chip("notebook-save", "Salvar caderno", cx).on_click(cx.listener(
                            |this, _, w, cx| this.save_notebook(w, cx),
                        )),
                    )
                    .child(
                        Self::chip("bundle-open", "Abrir investigação com capturas", cx).on_click(
                            cx.listener(|this, _, window, cx| this.open_bundle(window, cx)),
                        ),
                    )
                    .child(
                        Self::chip("bundle-save", "Salvar investigação com capturas", cx)
                            .on_click(cx.listener(|this, _, w, cx| this.save_bundle(w, cx))),
                    )
                    .when(!self.sources.is_empty(), |d| {
                        d.child(
                            Self::chip("bundle-restore-current", "Reabrir dump da seleção", cx)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.restore_current_dump(cx)),
                                ),
                        )
                    })
                    .when(self.sent_report.is_some(), |d| {
                        d.child(
                            Self::chip("codex-report-answer", "Relatório da última resposta", cx)
                                .on_click(
                                    cx.listener(|this, _, w, cx| this.export_report(true, w, cx)),
                                ),
                        )
                    }),
            )
            .child(div().text_color(t.text_muted).child(
                if self.notebook_revision != self.saved_revision {
                    "Há conversa ou anotações sem salvar. O caderno guarda o histórico em Markdown e JSON; revise antes de compartilhar."
                } else if self.saved_revision > 0 {
                    "Conversa e anotações salvas. Salve novamente após novas alterações."
                } else {
                    "Caderno em memória. Salve para conservar conversa, anotações e contextos depois de sair."
                },
            ))
            .child(div().text_color(t.text_muted).child(
                "Salvar com capturas copia os dumps integrais usados na análise, até 64 GiB, e os relatórios dos turnos com referências válidas, em texto aberto.",
            ))
    }
}

impl Render for CodexView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let header = div()
            .h(px(32.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.))
            .pl(px(10.))
            .pr(px(4.))
            .border_b_1()
            .border_color(t.divider)
            .child(
                div()
                    .size(px(8.))
                    .mr(px(6.))
                    .rounded_full()
                    .bg(if self.agent_ready { t.ok } else { t.text_muted }),
            )
            .child(
                div()
                    .flex_1()
                    .text_color(t.text_strong)
                    .child(if self.notes_open {
                        format!("Anotações ({})", self.notebook.notes().len())
                    } else {
                        "Conversa".into()
                    }),
            )
            .child(
                icon_button(
                    "notebook-tab",
                    "check_box",
                    "Hipóteses e anotações",
                    self.notes_open,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.notes_open = !this.notes_open;
                    cx.notify();
                })),
            )
            .child(
                icon_button("codex-reset", "add", "Nova investigação", false, cx)
                    .on_click(cx.listener(|this, _, _, cx| this.reset(cx))),
            )
            .child(
                icon_button(
                    "codex-settings",
                    "settings",
                    "Conexão, caderno e relatórios",
                    self.connection_open,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.connection_open = !this.connection_open;
                    cx.notify();
                })),
            );
        let linked = Self::bar(cx)
            .child(icon(
                "lan",
                14.,
                if self.evidence.is_some() {
                    t.accent
                } else {
                    t.text_muted
                },
            ))
            .child(match &self.evidence {
                Some(c) => div().flex_1().min_w_0().truncate().child(format!(
                    "Vinculada a captura {}… · {} pacotes anexados",
                    &c.sha256[..12],
                    c.packets.len()
                )),
                None => div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(t.text_muted)
                    .child("Sem captura vinculada"),
            })
            .when(self.following, |d| {
                d.child(icon("visibility", 14., t.accent))
                    .child(div().text_color(t.accent).child("acompanhando"))
            });
        // Account row: the agent status is the full reason, never hidden in a menu.
        let status_tip: gpui::SharedString = self.agent_status.clone().into();
        let account = Self::bar(cx)
            .py(px(4.))
            .child(
                div()
                    .id("codex-account")
                    .flex_1()
                    .min_w_0()
                    .text_color(t.text_muted)
                    .child(if self.agent_ready {
                        "Conta ChatGPT".to_owned()
                    } else if self.demo {
                        "Demonstração: conversa fictícia".to_owned()
                    } else {
                        self.agent_status.clone()
                    })
                    .tooltip(move |_, cx| cx.new(|_| Tip(status_tip.clone())).into()),
            )
            .child(if self.agent_ready {
                Self::chip("codex-logout", "Sair da conta", cx)
                    .when(self.demo || self.answering, |d| d.opacity(0.5))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.demo && !this.answering {
                            this.agent_command(Command::Logout, cx)
                        }
                    }))
            } else if self.agent.is_some() {
                Self::chip("codex-login", "Entrar com ChatGPT", cx)
                    .on_click(cx.listener(|this, _, _, cx| this.agent_command(Command::Login, cx)))
            } else {
                Self::chip(
                    "codex-connect",
                    if self.demo {
                        "Simular conexão"
                    } else {
                        "Conectar"
                    },
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| this.connect_agent(cx)))
            });
        let body = if self.notes_open {
            div()
                .id("codex-notes-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p(px(10.))
                .child(self.render_notes(cx))
                .with_scrollbar(cx)
        } else {
            self.render_conversation(cx)
        };
        let preview = self.preview.as_ref().map(|preview| {
            div()
                .flex_none()
                .mx(px(8.))
                .mt(px(8.))
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .rounded(px(4.))
                .border_1()
                .border_color(t.accent.opacity(0.5))
                .bg(t.editor)
                .text_size(px(11.5))
                .child(div().text_color(t.text_muted).child(format!(
                    "Texto exato do envio · {} bytes. Confira e envie de novo para confirmar.",
                    preview.len()
                )))
                .child(
                    div()
                        .id("codex-context-scroll")
                        .max_h(px(140.))
                        .overflow_y_scroll()
                        .font_family(MONO_FONT)
                        .text_size(px(11.))
                        .child(preview.clone())
                        .with_scrollbar(cx),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(
                            Self::chip("codex-copy", "Copiar contexto", cx)
                                .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                        )
                        .child(
                            Self::chip("codex-report-local", "Relatório local", cx).on_click(
                                cx.listener(|this, _, w, cx| this.export_report(false, w, cx)),
                            ),
                        ),
                )
        });
        let confirm = self.confirm_new.then(|| {
            div()
                .flex_none()
                .mx(px(8.))
                .mt(px(8.))
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .rounded(px(4.))
                .bg(t.editor)
                .text_size(px(11.5))
                .child("Iniciar outra investigação limpa o caderno em memória. Os arquivos já salvos são preservados.")
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(
                            Self::chip("notebook-confirm-reset", "Confirmar nova investigação", cx)
                                .on_click(cx.listener(|this, _, _, cx| this.reset(cx))),
                        )
                        .child(
                            Self::chip("notebook-keep", "Continuar nesta", cx).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.confirm_new = false;
                                    cx.notify();
                                }),
                            ),
                        ),
                )
        });
        // First use of following asks once, because it sends without per-turn review.
        let follow_confirm = self.follow_confirm.then(|| {
            div()
                .flex_none()
                .mx(px(8.))
                .mt(px(8.))
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .rounded(px(4.))
                .border_1()
                .border_color(t.accent.opacity(0.5))
                .bg(t.editor)
                .text_size(px(11.5))
                .child("Acompanhar a captura de diagnóstico: a cada 30 s com pacotes novos, o NetLane envia sozinho ao Codex os agregados da captura e os metadados dos pacotes mais recentes (endereços, portas, protocolos e flags), sem payload. Não há revisão antes de cada envio; cada um fica registrado no caderno. Desligue no olho a qualquer momento.")
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(
                            Self::chip("follow-confirm", "Ligar acompanhamento", cx)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.start_following();
                                    cx.notify();
                                })),
                        )
                        .child(Self::chip("follow-cancel", "Cancelar", cx).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.follow_confirm = false;
                                cx.notify();
                            }),
                        )),
                )
        });
        let ready = self.evidence.is_some() && !self.answering;
        let send = if self.answering {
            icon_button("codex-interrupt", "stop", "Interromper resposta", false, cx).on_click(
                cx.listener(|this, _, _, cx| {
                    this.agent_command(
                        Command::Cancel {
                            conversation: this.conversation,
                        },
                        cx,
                    );
                    this.status =
                        "Interrupção solicitada; aguardando o encerramento do turno.".into();
                    cx.notify();
                }),
            )
        } else if self.preview.is_some() || self.evidence.is_none() || self.following {
            icon_button(
                "codex-send",
                "send",
                if self.following && self.evidence.is_some() {
                    "Enviar com a foto mais recente da captura acompanhada (Enter)"
                } else if self.evidence.is_none() {
                    "Enviar pergunta, sem captura anexada (Enter)"
                } else if self.demo {
                    "Simular envio (Enter)"
                } else {
                    "Enviar o texto revisado (Enter)"
                },
                self.preview.is_some(),
                cx,
            )
            .on_click(cx.listener(|this, _, _, cx| this.send(cx)))
        } else {
            icon_button(
                "codex-review",
                "send",
                "Revisar o texto do envio (Enter)",
                false,
                cx,
            )
            .when(!ready, |d| d.opacity(0.5))
            .on_click(cx.listener(|this, _, _, cx| this.review(cx)))
        };
        let effort = self
            .model
            .as_ref()
            .filter(|_| self.agent_ready && !self.models.is_empty())
            .map(|choice| {
                div()
                    .id("codex-effort")
                    .h(px(24.))
                    .flex_none()
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .rounded(px(4.))
                    .text_size(px(11.5))
                    .text_color(t.text_muted)
                    .cursor_pointer()
                    .hover(|d| d.bg(t.hover))
                    .tooltip(|_, cx| {
                        cx.new(|_| Tip("Esforço de raciocínio (clique para trocar)".into()))
                            .into()
                    })
                    .child(effort_label(&choice.effort).to_owned())
                    .child("▾")
                    .on_click(cx.listener(|this, _, _, cx| this.cycle_effort(cx)))
            });
        let model = (self.agent_ready && !self.models.is_empty()).then(|| {
            div()
                .id("codex-model")
                .h(px(24.))
                .min_w_0()
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(2.))
                .rounded(px(4.))
                .text_size(px(11.5))
                .text_color(t.text_muted)
                .cursor_pointer()
                .hover(|d| d.bg(t.hover))
                .tooltip(|_, cx| cx.new(|_| Tip("Trocar de modelo".into())).into())
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(self.model.as_ref().map_or("—".into(), |m| m.model.clone())),
                )
                .child("▾")
                .on_click(cx.listener(|this, _, _, cx| this.cycle_model(cx)))
        });
        let status_tip: gpui::SharedString = self.status.clone().into();
        let composer = div()
            .flex_none()
            .border_t_1()
            .border_color(t.divider)
            .px(px(8.))
            .pt(px(8.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .when_some(self.evidence.as_ref(), |d, context| {
                d.child(
                    div().flex().flex_wrap().gap(px(4.)).children(
                        context.packets.iter().map(|packet| {
                            let number = packet.number;
                            Self::chip(
                                ("remove-evidence", number as usize),
                                &if self.answering {
                                    format!("#{number}")
                                } else {
                                    format!("#{number} ×")
                                },
                                cx,
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_packet(number, cx)
                            }))
                        }),
                    ),
                )
            })
            .when(!self.status.is_empty(), |d| {
                d.child(
                    div()
                        .id("codex-status")
                        .truncate()
                        .text_size(px(11.))
                        .text_color(t.text_muted)
                        .tooltip(move |_, cx| cx.new(|_| Tip(status_tip.clone())).into())
                        .child(self.status.clone()),
                )
            })
            .when(self.problem_open, |d| {
                d.child(Self::field(self.problem.clone(), cx))
            })
            .child(Self::field(self.question.clone(), cx).min_h(px(44.)))
            .child(
                div()
                    .h(px(34.))
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        icon_button(
                            "codex-follow",
                            "visibility",
                            if self.following {
                                "Codex acompanhando a captura de diagnóstico (clique para parar)"
                            } else {
                                "Deixar o Codex acompanhar a captura de diagnóstico"
                            },
                            self.following,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_follow(cx))),
                    )
                    .child(
                        icon_button(
                            "problem-toggle",
                            "warning",
                            "Problema observado neste computador",
                            self.problem_open,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.problem_open = !this.problem_open;
                            cx.notify();
                        })),
                    )
                    .when(ready, |d| {
                        d.child(icon_button("ask-hypotheses", "search", "Explorar hipóteses", false, cx).on_click(cx.listener(|this, _, _, cx| this.suggest("Quais hipóteses concorrentes temos? Separe evidências a favor, contra e o que ainda falta observar.", cx))))
                            .child(icon_button("ask-next", "refresh", "Próxima verificação", false, cx).on_click(cx.listener(|this, _, _, cx| this.suggest("Qual seria a próxima verificação para reduzir a incerteza? Explique o que esperar em cada hipótese.", cx))))
                            .child(icon_button("ask-resume", "save", "Preparar resumo para retomar", false, cx).on_click(cx.listener(|this, _, _, cx| this.prepare_resume(cx))))
                    })
                    .child(div().flex_1())
                    .children(model)
                    .children(effort)
                    .child(
                        div()
                            .px(px(6.))
                            .text_size(px(11.5))
                            .text_color(t.text_muted)
                            .child(if self.demo {
                                "Codex · simulado"
                            } else {
                                "Codex · ChatGPT"
                            }),
                    )
                    .child(send),
            )
            .when(self.bundle_busy, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.))
                        .pb(px(6.))
                        .text_size(px(11.5))
                        .child(format!(
                            "Capturas conferidas: {} · {:.1} / {:.1} MiB",
                            self.bundle_progress.files.load(Ordering::Relaxed),
                            self.bundle_progress.bytes.load(Ordering::Relaxed) as f64 / 1048576.,
                            self.bundle_progress.total.load(Ordering::Relaxed) as f64 / 1048576.
                        ))
                        .child(Self::chip("bundle-cancel", "Cancelar", cx).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.bundle_cancel.store(true, Ordering::Relaxed);
                                this.status =
                                    "Cancelamento solicitado; aguardando encerrar a leitura/gravação."
                                        .into();
                                cx.notify();
                            }),
                        )),
                )
            });
        div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(t.panel)
            .text_size(px(12.5))
            .child(header)
            .child(linked)
            .child(account)
            .when(self.connection_open, |d| d.child(self.render_options(cx)))
            .child(body)
            .children(preview)
            .children(confirm)
            .children(follow_confirm)
            .child(composer)
    }
}
