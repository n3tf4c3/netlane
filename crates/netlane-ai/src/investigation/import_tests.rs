use super::*;
use serde_json::{Value, json};
use tests::draft;

fn sample() -> Result<Notebook> {
    let mut notebook = Notebook::default();
    let draft = draft();
    notebook.record(7, draft.clone())?;
    notebook.add_note("Conferir recusa", draft.context, Some(1))?;
    notebook.cycle_note(1)?;
    Ok(notebook)
}

// Deliberately rehash modified imports: a consistent receipt is not authorship
// or semantic validation. Only known local fixture files are written.
fn write_bundle(directory: &Path, bundle: &Value) -> Result<()> {
    fs::create_dir(directory)?;
    let files = [
        ("investigation.json", serde_json::to_vec(bundle)?),
        ("investigation.md", b"Fixture local".to_vec()),
    ];
    let receipt = Receipt {
        schema: 1,
        turns: bundle["notebook"]["turns"].as_array().map_or(0, Vec::len),
        notes: bundle["notebook"]["notes"].as_array().map_or(0, Vec::len),
        files: files
            .iter()
            .map(|(name, bytes)| FileHash {
                file: (*name).into(),
                bytes: bytes.len(),
                sha256: format!("{:x}", Sha256::digest(bytes)),
            })
            .collect(),
    };
    for (name, bytes) in files {
        fs::write(directory.join(name), bytes)?;
    }
    fs::write(
        directory.join("receipt.json"),
        serde_json::to_vec(&receipt)?,
    )?;
    Ok(())
}

#[test]
fn imported_notebook_roundtrips_and_resumes_without_rewriting_source() -> Result<()> {
    let original = sample()?;
    let temp = tempfile::tempdir()?;
    let folder = temp.path().join("original");
    original.save(&folder)?;
    let before = fs::read(folder.join("investigation.json"))?;
    let mut reopened = Notebook::load(&folder)?;
    assert_eq!(
        serde_json::to_value(&original)?,
        serde_json::to_value(&reopened)?
    );
    assert_eq!(reopened.next_conversation(2)?, 8);
    assert_eq!(reopened.next_conversation(20)?, 21);
    assert!(reopened.next_conversation(u64::MAX).is_err());
    reopened.record(8, draft())?;
    reopened.cycle_note(1)?;
    reopened.save(&temp.path().join("resumed"))?;
    assert_eq!(fs::read(folder.join("investigation.json"))?, before);
    assert_eq!(reopened.notes()[0].state_history.len(), 3);
    Ok(())
}

#[test]
fn legacy_import_does_not_invent_creation_times_or_past_transitions() -> Result<()> {
    let mut bundle = json!({"schema":1, "notebook":sample()?});
    for turn in bundle["notebook"]["turns"].as_array_mut().unwrap() {
        turn.as_object_mut().unwrap().remove("created_unix_seconds");
    }
    for note in bundle["notebook"]["notes"].as_array_mut().unwrap() {
        note.as_object_mut().unwrap().remove("created_unix_seconds");
        note.as_object_mut().unwrap().remove("state_history");
    }
    let temp = tempfile::tempdir()?;
    let folder = temp.path().join("legacy");
    write_bundle(&folder, &bundle)?;
    let mut reopened = Notebook::load(&folder)?;
    assert!(reopened.turns()[0].created_unix_seconds.is_none());
    assert!(reopened.notes()[0].state_history.is_empty());
    reopened.cycle_note(1)?;
    assert!(reopened.notes()[0].created_unix_seconds.is_none());
    assert_eq!(
        reopened.notes()[0].state_history[0].state,
        NoteState::Confirmed
    );
    assert_eq!(
        reopened.notes()[0].state_history[1].state,
        NoteState::Discarded
    );
    reopened.save(&temp.path().join("new"))?;
    Ok(())
}

#[test]
fn import_rechecks_references_even_with_a_rehashed_claim_of_validity() -> Result<()> {
    let mut bundle = json!({"schema":1,"notebook":sample()?});
    bundle["notebook"]["turns"][0]["draft"]["response"]["text"] =
        json!(format!("[[packet:{}:99]]", "A".repeat(64)));
    bundle["notebook"]["turns"][0]["references_valid"] = json!(true);
    let temp = tempfile::tempdir()?;
    let folder = temp.path().join("foreign");
    write_bundle(&folder, &bundle)?;
    let reopened = Notebook::load(&folder)?;
    assert!(!reopened.turns()[0].references_valid);
    assert!(reopened.turns()[0].draft.validate().is_err());
    let summary = reopened.resume_question(&draft().context, "Retomar?")?;
    assert!(summary.contains("Resposta omitida"));
    assert!(!summary.contains(":99]]"));
    Ok(())
}

#[test]
fn import_rejects_inconsistent_origin_ids_history_and_excessive_arrays() -> Result<()> {
    let valid = json!({"schema":1,"notebook":sample()?});
    let temp = tempfile::tempdir()?;
    for case in 0..8 {
        let mut bundle = valid.clone();
        match case {
            0 => bundle["notebook"]["notes"][0]["context"]["sha256"] = json!("B".repeat(64)),
            1 => bundle["notebook"]["turns"][0]["id"] = json!(2),
            2 => bundle["notebook"]["notes"][0]["state"] = json!("Discarded"),
            3 => {
                bundle["notebook"]["notes"][0]["state_history"][1]["state"] = json!("Investigating")
            }
            4 => bundle["schema"] = json!(2),
            5 => {
                let turn = bundle["notebook"]["turns"][0].clone();
                bundle["notebook"]["turns"] = json!(vec![turn; MAX_TURNS + 1]);
            }
            6 => {
                let note = bundle["notebook"]["notes"][0].clone();
                bundle["notebook"]["notes"] = json!(vec![note; MAX_NOTES + 1]);
            }
            _ => {
                let change = bundle["notebook"]["notes"][0]["state_history"][0].clone();
                bundle["notebook"]["notes"][0]["state_history"] =
                    json!(vec![change; MAX_NOTE_CHANGES + 1]);
            }
        }
        let folder = temp.path().join(format!("case-{case}"));
        write_bundle(&folder, &bundle)?;
        assert!(Notebook::load(&folder).is_err(), "case {case}");
    }
    Ok(())
}

#[test]
fn import_refuses_receipt_paths_duplicates_sizes_counts_and_changed_files() -> Result<()> {
    let temp = tempfile::tempdir()?;
    assert!(Notebook::load(Path::new("relative")).is_err());
    for case in 0..7 {
        let folder = temp.path().join(format!("case-{case}"));
        sample()?.save(&folder)?;
        let receipt_path = folder.join("receipt.json");
        let mut receipt: Value = serde_json::from_slice(&fs::read(&receipt_path)?)?;
        match case {
            0 => receipt["files"][0]["file"] = json!("../outside.json"),
            1 => receipt["files"][1] = receipt["files"][0].clone(),
            2 => receipt["files"][0]["bytes"] = json!(MAX_NOTEBOOK_BYTES + 1),
            3 => receipt["turns"] = json!(2),
            4 => receipt["files"][0]["sha256"] = json!("0".repeat(64)),
            5 => fs::write(folder.join("investigation.md"), "changed")?,
            _ => {
                let mut bytes = fs::read(folder.join("investigation.json"))?;
                bytes.push(b' ');
                fs::write(folder.join("investigation.json"), bytes)?;
            }
        }
        fs::write(receipt_path, serde_json::to_vec(&receipt)?)?;
        assert!(Notebook::load(&folder).is_err(), "case {case}");
    }
    Ok(())
}

#[test]
fn note_history_is_bounded_without_mutating_on_limit() -> Result<()> {
    let mut notebook = sample()?;
    while notebook.notes()[0].state_history.len() < MAX_NOTE_CHANGES {
        notebook.cycle_note(1)?;
    }
    let before = serde_json::to_value(&notebook)?;
    assert!(notebook.cycle_note(1).is_err());
    assert_eq!(serde_json::to_value(&notebook)?, before);
    notebook.validate()?;
    Ok(())
}

#[test]
fn resume_selects_recent_same_capture_excerpts_and_keeps_question() -> Result<()> {
    let mut notebook = Notebook::default();
    for index in 0..4 {
        let mut item = draft();
        item.question = format!("pergunta-{index}");
        item.response.as_mut().unwrap().text = "é".repeat(10_000);
        notebook.record(1, item)?;
        notebook.add_note(&format!("nota-{index}"), draft().context, None)?;
    }
    let mut foreign = draft();
    foreign.context.sha256 = "B".repeat(64);
    foreign.question = "foreign-question".into();
    notebook.record(2, foreign.clone())?;
    notebook.add_note("foreign-note", foreign.context, None)?;
    let question = "Qual próxima verificação?";
    let summary = notebook.resume_question(&draft().context, question)?;
    assert!(summary.len() <= crate::MAX_PROBLEM_BYTES);
    assert!(
        !summary.contains("pergunta-0")
            && !summary.contains("pergunta-1")
            && !summary.contains("nota-0")
            && !summary.contains("foreign")
    );
    assert!(
        summary.contains("pergunta-2")
            && summary.contains("pergunta-3")
            && summary.contains("nota-1")
            && summary.ends_with(question)
    );
    assert!(
        notebook
            .resume_question(&draft().context, &"x".repeat(513))
            .is_err()
    );
    assert!(
        Notebook::default()
            .resume_question(&draft().context, question)
            .is_err()
    );
    Ok(())
}

#[test]
fn utc_registration_dates_cover_epoch_leap_centuries_and_upper_bound() {
    assert_eq!(registration_label(0), "1970-01-01 00:00:00 UTC");
    assert_eq!(registration_label(951_782_400), "2000-02-29 00:00:00 UTC");
    assert_eq!(registration_label(1_709_251_200), "2024-03-01 00:00:00 UTC");
    assert_eq!(
        registration_label(253_402_300_799),
        "9999-12-31 23:59:59 UTC"
    );
    assert_eq!(registration_label(u64::MAX), "Horário inválido");
}
