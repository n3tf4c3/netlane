use super::*;

fn excerpt(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

impl Notebook {
    /// Local excerpts, not AI inference. Only history from the selected capture.
    pub fn resume_question(&self, context: &Context, question: &str) -> Result<String> {
        context.validate()?;
        ensure!(
            question.len() <= 512,
            "Para acrescentar o resumo, use uma pergunta de até 512 bytes."
        );
        let turns: Vec<_> = self
            .turns
            .iter()
            .filter(|turn| turn.draft.context.sha256 == context.sha256)
            .rev()
            .take(2)
            .collect();
        let notes: Vec<_> = self
            .notes
            .iter()
            .filter(|note| note.context.sha256 == context.sha256)
            .rev()
            .take(3)
            .collect();
        ensure!(
            !turns.is_empty() || !notes.is_empty(),
            "Não há histórico desta captura para resumir."
        );
        let mut summary = format!(
            "Resumo local em trechos da investigação da captura {}. Inclui até 2 turnos e 3 anotações recentes; não é o histórico completo nem prova de diagnóstico. As seleções antigas são referências históricas, não anexos novos. Use somente o contexto estruturado atual como evidência e peça os anexos que faltarem. ",
            context.sha256
        );
        for turn in turns.into_iter().rev() {
            summary.push_str(&format!(
                "[Turno {}; conversa {}; seleção {:?}. Pergunta (trecho): {}. ",
                turn.id,
                turn.conversation,
                turn.draft
                    .context
                    .packets
                    .iter()
                    .map(|p| p.number)
                    .collect::<Vec<_>>(),
                excerpt(&turn.draft.question, 192)
            ));
            if let Some(response) = &turn.draft.response {
                summary.push_str(&format!(
                    "Estado (trecho): {}. ",
                    excerpt(&response.status, 96)
                ));
                if turn.references_valid {
                    summary.push_str(&format!(
                        "Resposta da IA (trecho, hipótese a revisar): {}",
                        excerpt(&response.text, 512)
                    ));
                } else {
                    summary.push_str(
                        "Resposta omitida por referências inválidas; confira o registro local.",
                    );
                }
            }
            summary.push_str("] ");
        }
        for note in notes.into_iter().rev() {
            summary.push_str(&format!(
                "[Anotação {}; estado registrado pelo autor: {}; trecho: {}] ",
                note.id,
                note.state.label(),
                excerpt(&note.text, 160)
            ));
        }
        summary.push_str(&format!(
            "Pergunta atual: {}",
            if question.trim().is_empty() {
                "Como podemos retomar a investigação e qual evidência falta conferir?"
            } else {
                question.trim()
            }
        ));
        // The editable GPUI question is a single-line field.
        summary = summary.replace(['\r', '\n'], " ");
        ensure!(
            summary.len() <= crate::MAX_PROBLEM_BYTES,
            "O resumo excede 4 KiB. Selecione um contexto menor antes de retomar."
        );
        Ok(summary)
    }
}
