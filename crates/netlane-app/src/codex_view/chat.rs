use super::*;
impl Render for CodexView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let mut view = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .min_w_0()
            .child(
                div()
                    .text_size(px(22.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Investigar com Codex"),
            )
            .child(
                div()
                    .text_color(t.text_muted)
                    .child("Converse, confronte hipóteses e anote o que precisa verificar."),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .when(!self.agent_ready, |d| {
                        d.child(
                            Self::button(
                                "codex-connect",
                                if self.demo {
                                    "Simular conexão"
                                } else {
                                    "Conectar Codex"
                                },
                                cx,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.connect_agent(cx))),
                        )
                    })
                    .when(self.agent.is_some() && !self.agent_ready, |d| {
                        d.child(
                            Self::button("codex-login", "Entrar com ChatGPT", cx).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.agent_command(Command::Login, cx)
                                }),
                            ),
                        )
                    })
                    .when(self.agent_ready && !self.models.is_empty(), |d| {
                        d.child(
                            Self::button(
                                "codex-model",
                                &format!(
                                    "Modelo: {}",
                                    self.model.as_ref().map_or("—", |m| m.model.as_str())
                                ),
                                cx,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.cycle_model(cx))),
                        )
                    })
                    .child(
                        Self::button("codex-settings", "Conexão", cx).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.connection_open = !this.connection_open;
                                cx.notify();
                            },
                        )),
                    )
                    .child(Self::button("notebook-open", "Abrir caderno", cx).on_click(
                        cx.listener(|this, _, window, cx| this.open_notebook(window, cx)),
                    ))
                    .child(
                        Self::button("bundle-open", "Abrir investigação com capturas", cx)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_bundle(window, cx)),
                            ),
                    )
                    .when(!self.sources.is_empty(), |row| {
                        row.child(
                            Self::button("bundle-restore-current", "Reabrir dump da seleção", cx)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.restore_current_dump(cx)),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(t.text_muted)
                    .child(if self.demo {
                        "Demonstração: conversa fictícia, sem provedor real."
                    } else if self.agent_ready {
                        "Codex conectado · o próximo envio usa sua conta."
                    } else {
                        "Conecte sua conta para enviar as evidências revisadas."
                    }),
            );
        if let Some(context) = &self.evidence {
            view=view.child(div().p(px(10.)).bg(t.panel).rounded(px(5.)).flex().flex_col().gap(px(4.))
                .child(format!("Conversa {} · contexto {}…",self.conversation,&context.sha256[..12]))
                .child(format!("{} · {} pacotes no dump · seleção {:?}",context.format,context.total_packets,context.packets.iter().map(|p|p.number).collect::<Vec<_>>()))
                .child(div().text_size(px(11.)).text_color(t.text_muted).child("Somente metadados e agregados selecionados. Novos segmentos não são enviados automaticamente."))
                .when(!self.answering, |d| {
                    d.child(div().flex().flex_wrap().gap(px(4.)).children(context.packets.iter().map(|packet| {
                        let number = packet.number;
                        Self::button(("remove-evidence", number as usize), &format!("Remover #{number}"), cx)
                            .on_click(cx.listener(move |this, _, _, cx| this.remove_packet(number, cx)))
                    })))
                }));
        } else {
            view=view.child(div().text_color(t.text_muted).child("Selecione pacotes e use Preparar análise para Codex para anexar as evidências ao chat."));
        }
        view = view.child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .child(
                    Self::button(
                        "notebook-tab",
                        &format!(
                            "{} ({})",
                            if self.notes_open {
                                "Voltar ao chat"
                            } else {
                                "Anotações"
                            },
                            self.notebook.notes().len()
                        ),
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.notes_open = !this.notes_open;
                        cx.notify();
                    })),
                )
                .child(
                    Self::button("problem-toggle", "Problema observado", cx).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.problem_open = !this.problem_open;
                            cx.notify();
                        },
                    )),
                ),
        );
        if self.problem_open {
            view = view.child(self.problem.clone());
        }
        if self.notes_open {
            view = view.child(self.render_notes(cx));
        } else {
            view = view.child(self.render_chat(
                (f32::from(window.viewport_size().height) * 0.22).clamp(140., 260.),
                cx,
            ));
        }
        if self.evidence.is_some() && !self.notes_open {
            view=view.child(div().flex().flex_col().gap(px(7.))
                .child(div().text_size(px(12.)).child("Sua próxima pergunta")).child(self.question.clone())
                .child(div().flex().flex_wrap().gap(px(6.))
                    .child(Self::button("ask-hypotheses","Explorar hipóteses",cx).on_click(cx.listener(|this,_,_,cx|this.suggest("Quais hipóteses concorrentes temos? Separe evidências a favor, contra e o que ainda falta observar.",cx))))
                    .child(Self::button("ask-next","Próxima verificação",cx).on_click(cx.listener(|this,_,_,cx|this.suggest("Qual seria a próxima verificação para reduzir a incerteza? Explique o que esperar em cada hipótese.",cx))))
                    .child(Self::button("ask-resume","Preparar resumo para retomar",cx).on_click(cx.listener(|this,_,_,cx|this.prepare_resume(cx)))))
                .child(div().flex().flex_wrap().gap(px(6.))
                    .child(Self::button("codex-review","Revisar contexto",cx).on_click(cx.listener(|this,_,_,cx|this.review(cx))))
                    .child(Self::button("codex-send",if self.answering {"Analisando…"}else if self.demo {"Simular envio"}else{"Enviar ao Codex"},cx).on_click(cx.listener(|this,_,_,cx|this.send(cx))))
                    .when(self.answering,|d|d.child(Self::button("codex-interrupt","Interromper",cx).on_click(cx.listener(|this,_,_,cx|{this.agent_command(Command::Cancel{conversation:this.conversation},cx);this.status="Interrupção solicitada; aguardando o encerramento do turno.".into();cx.notify();}))))));
        }
        view = view.child(
            div()
                .text_size(px(12.))
                .text_color(t.accent)
                .child(self.status.clone()),
        );
        if self.bundle_busy {
            view = view.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .items_center()
                    .child(format!(
                        "Capturas conferidas: {} · {:.1} / {:.1} MiB processados",
                        self.bundle_progress.files.load(Ordering::Relaxed),
                        self.bundle_progress.bytes.load(Ordering::Relaxed) as f64 / 1048576.,
                        self.bundle_progress.total.load(Ordering::Relaxed) as f64 / 1048576.
                    ))
                    .child(
                        Self::button("bundle-cancel", "Cancelar operação", cx).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.bundle_cancel.store(true, Ordering::Relaxed);
                                this.status =
                                "Cancelamento solicitado; aguardando encerrar a leitura/gravação."
                                    .into();
                                cx.notify();
                            }),
                        ),
                    ),
            );
        }
        if let Some(preview) = &self.preview {
            view = view.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(format!("Texto do próximo envio · {} bytes", preview.len()))
                    .child(
                        div()
                            .id("codex-context-scroll")
                            .max_h(px(160.))
                            .overflow_y_scroll()
                            .p(px(10.))
                            .bg(t.panel)
                            .font_family(MONO_FONT)
                            .text_size(px(11.))
                            .child(preview.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.))
                            .child(
                                Self::button("codex-copy", "Copiar contexto", cx)
                                    .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                            )
                            .child(
                                Self::button("codex-report-local", "Relatório local", cx).on_click(
                                    cx.listener(|this, _, w, cx| this.export_report(false, w, cx)),
                                ),
                            ),
                    ),
            );
        }
        view=view.child(div().flex().flex_wrap().gap(px(6.))
                .child(Self::button("notebook-save","Salvar caderno",cx).on_click(cx.listener(|this,_,w,cx|this.save_notebook(w,cx))))
                .child(Self::button("bundle-save","Salvar investigação com capturas",cx).on_click(cx.listener(|this,_,w,cx|this.save_bundle(w,cx))))
                .when(self.sent_report.is_some(),|d|d.child(Self::button("codex-report-answer","Relatório da última resposta",cx).on_click(cx.listener(|this,_,w,cx|this.export_report(true,w,cx)))))
                .child(Self::button("codex-reset","Nova investigação",cx).on_click(cx.listener(|this,_,_,cx|this.reset(cx)))))
            .child(div().text_size(px(11.)).text_color(t.text_muted).child("Salvar com capturas copia os dumps integrais usados na análise, até 64 GiB, e os relatórios dos turnos com referências válidas. O pacote permanece em texto aberto."))
            .child(div().text_size(px(11.)).text_color(t.text_muted).child(if self.notebook_revision!=self.saved_revision {"Há conversa ou anotações sem salvar. O caderno guarda o histórico em Markdown e JSON; revise antes de compartilhar."}else if self.saved_revision>0 {"Conversa e anotações salvas. Salve novamente após novas alterações."}else{"Caderno em memória. Salve para conservar conversa, anotações e contextos depois de sair."}));
        if self.confirm_new {
            view=view.child(div().p(px(10.)).bg(t.panel).flex().flex_col().gap(px(6.))
                .child("Iniciar outra investigação limpa o caderno em memória. Os arquivos já salvos são preservados.")
                .child(Self::button("notebook-confirm-reset","Confirmar nova investigação",cx).on_click(cx.listener(|this,_,_,cx|this.reset(cx))))
                .child(Self::button("notebook-keep","Continuar nesta investigação",cx).on_click(cx.listener(|this,_,_,cx|{this.confirm_new=false;cx.notify();}))));
        }
        if self.connection_open {
            let busy = self.probe.as_ref().is_some_and(|p| !p.state().done);
            view=view.child(div().p(px(12.)).bg(t.panel).flex().flex_col().gap(px(8.))
                .child(self.agent_status.clone())
                .child(div().flex().flex_wrap().gap(px(6.))
                    .child(Self::button("codex-check",if self.demo {"Simular verificação"}else{"Verificar conexão"},cx).on_click(cx.listener(|this,_,_,cx|this.check_connection(cx))))
                    .when(busy,|d|d.child(Self::button("codex-cancel-probe","Cancelar verificação",cx).on_click(cx.listener(|this,_,_,cx|{if let Some(probe)=&this.probe {probe.cancel();}cx.notify();}))))
                    .when(self.agent.is_some()&&!self.agent_ready,|d|d.child(Self::button("codex-login-cancel","Cancelar login",cx).on_click(cx.listener(|this,_,_,cx|this.agent_command(Command::CancelLogin,cx)))))
                    .when(self.agent.is_some()&&self.agent_ready&&!self.answering,|d|d.child(Self::button("codex-logout","Sair da conta",cx).on_click(cx.listener(|this,_,_,cx|this.agent_command(Command::Logout,cx)))))
                    .when(!self.demo,|d|d.child(Self::button("codex-exe","Caminho do Codex",cx).on_click(cx.listener(|this,_,_,cx|{this.edit_executable = !this.edit_executable;cx.notify();})))))
                .when(self.edit_executable,|d|d.child(self.executable.clone()))
                .child(self.probe_state.detail.clone())
                .child(div().text_size(px(11.)).text_color(t.text_muted).child("A verificação consulta metadados, sem inferência. A conversa usa perfil próprio e não executa comandos no computador.")));
        }
        view
    }
}
