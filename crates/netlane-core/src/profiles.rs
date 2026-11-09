//! Named rule sets ("Gaming", "Trabalho"...). `netlane-rules.json` keeps holding the
//! rules of the active profile, in the format shared with the agent and the .NET
//! reference; this store keeps a copy of every profile. The rules file wins for the
//! active profile: its slot here is refreshed on every save and switch.
use crate::{
    RoutingPolicy,
    persistence::{atomic_write_checked, exclusive_file, read_bounded, revision, without_bom},
    validate_policies,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const PROFILES_FILE: &str = "netlane-profiles.json";
pub const DEFAULT_PROFILE: &str = "Padrão";
pub const MAX_PROFILES: usize = 12;
pub const MAX_PROFILE_NAME: usize = 40;
/// Same ceiling as the rules file (the checked atomic write re-reads within it).
pub const MAX_PROFILES_BYTES: u64 = crate::persistence::MAX_JSON_BYTES;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    pub policies: Vec<RoutingPolicy>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profiles {
    pub active: String,
    pub profiles: Vec<Profile>,
}
impl Profiles {
    /// State before any profile was created: one profile with the current rules.
    pub fn implicit(current: &[RoutingPolicy]) -> Self {
        Self {
            active: DEFAULT_PROFILE.to_owned(),
            profiles: vec![Profile {
                name: DEFAULT_PROFILE.to_owned(),
                policies: current.to_vec(),
            }],
        }
    }
    pub fn names(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.name.clone()).collect()
    }
    fn position(&self, name: &str) -> Option<usize> {
        let key = name.trim().to_lowercase();
        self.profiles
            .iter()
            .position(|p| p.name.trim().to_lowercase() == key)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.profiles.is_empty(), "Nenhum perfil cadastrado.");
        ensure!(
            self.profiles.len() <= MAX_PROFILES,
            "Limite de {MAX_PROFILES} perfis excedido."
        );
        let mut names = std::collections::HashSet::new();
        for profile in &self.profiles {
            check_name(&profile.name)?;
            ensure!(
                names.insert(profile.name.trim().to_lowercase()),
                "Há mais de um perfil chamado {}.",
                profile.name
            );
            validate_policies(&profile.policies)
                .with_context(|| format!("Perfil {}", profile.name))?;
        }
        ensure!(
            self.position(&self.active).is_some(),
            "O perfil ativo {} não existe.",
            self.active
        );
        Ok(())
    }
    /// Refreshes the active profile's copy with the saved rules.
    pub fn sync_active(&mut self, current: &[RoutingPolicy]) {
        if let Some(i) = self.position(&self.active) {
            self.profiles[i].policies = current.to_vec();
        }
    }
    /// New profile holding a copy of the current rules; it becomes the active one, so
    /// the rules file needs no change.
    pub fn create(&self, name: &str, current: &[RoutingPolicy]) -> Result<Self> {
        let name = name.trim();
        check_name(name)?;
        ensure!(
            self.position(name).is_none(),
            "Já existe um perfil chamado {name}."
        );
        let mut next = self.clone();
        next.sync_active(current);
        next.profiles.push(Profile {
            name: name.to_owned(),
            policies: current.to_vec(),
        });
        next.active = name.to_owned();
        next.validate()?;
        Ok(next)
    }
    /// The store after switching and the rules the rules file must receive.
    pub fn switch(
        &self,
        target: &str,
        current: &[RoutingPolicy],
    ) -> Result<(Self, Vec<RoutingPolicy>)> {
        let i = self
            .position(target)
            .with_context(|| format!("Perfil {target} não encontrado."))?;
        let mut next = self.clone();
        next.sync_active(current);
        next.active = next.profiles[i].name.clone();
        next.validate()?;
        let policies = next.profiles[i].policies.clone();
        Ok((next, policies))
    }
    /// Removes an inactive profile.
    pub fn delete(&self, name: &str) -> Result<Self> {
        let i = self
            .position(name)
            .with_context(|| format!("Perfil {name} não encontrado."))?;
        ensure!(
            Some(i) != self.position(&self.active),
            "Troque de perfil antes de excluir o perfil ativo."
        );
        let mut next = self.clone();
        next.profiles.remove(i);
        next.validate()?;
        Ok(next)
    }
}
fn check_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    ensure!(!trimmed.is_empty(), "Dê um nome ao perfil.");
    ensure!(
        trimmed.chars().count() <= MAX_PROFILE_NAME,
        "Use até {MAX_PROFILE_NAME} caracteres no nome do perfil."
    );
    ensure!(
        !trimmed.chars().any(char::is_control),
        "O nome do perfil não pode ter caracteres de controle."
    );
    Ok(())
}

#[derive(Clone, Debug)]
pub struct ProfilesSnapshot {
    pub profiles: Profiles,
    pub revision: String,
}
/// Same revision control and backup as the rules file.
#[derive(Clone)]
pub struct ProfileFile {
    pub path: PathBuf,
}
impl ProfileFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    /// `None` while no profile was ever created.
    pub fn load(&self) -> Result<Option<ProfilesSnapshot>> {
        let Some(bytes) = read_bounded(&self.path, MAX_PROFILES_BYTES)? else {
            return Ok(None);
        };
        let profiles: Profiles =
            serde_json::from_slice(without_bom(&bytes)).context("JSON de perfis inválido.")?;
        profiles.validate()?;
        Ok(Some(ProfilesSnapshot {
            profiles,
            revision: revision(&bytes),
        }))
    }
    pub fn save(&self, profiles: &Profiles, expected: Option<&str>) -> Result<String> {
        profiles.validate()?;
        let bytes = serde_json::to_vec_pretty(profiles)?;
        if bytes.len() as u64 > MAX_PROFILES_BYTES {
            bail!("Os perfis serializados excedem o limite de 4 MiB.");
        }
        let parent = self.path.parent().context("Pasta de perfis inválida.")?;
        std::fs::create_dir_all(parent)?;
        let _lock = exclusive_file(&self.path.with_extension("json.lock"))?;
        let current = read_bounded(&self.path, MAX_PROFILES_BYTES)?.map(|b| revision(&b));
        if current.as_deref() != expected {
            bail!("Os perfis mudaram desde a leitura. Reabra o NetLane antes de alterá-los.");
        }
        atomic_write_checked(&self.path, &bytes, true, expected)?;
        Ok(revision(&bytes))
    }
}

/// Files after a successful switch.
#[derive(Debug)]
pub struct Switched {
    pub store: Profiles,
    pub store_revision: String,
    pub policies: Vec<RoutingPolicy>,
    pub rules_revision: String,
}
#[derive(Debug)]
pub enum SwitchError {
    /// Nothing was written.
    Refused(anyhow::Error),
    /// The store write failed and the previous rules were written back.
    Restored {
        error: anyhow::Error,
        rules_revision: String,
    },
    /// The rules file holds the target profile but the store still names the previous
    /// one. Callers must stop writing profiles until the user checks them.
    Inconsistent {
        error: anyhow::Error,
        restore: anyhow::Error,
        policies: Vec<RoutingPolicy>,
        rules_revision: String,
    },
}
/// Switches profiles on disk: the rules file first (its revision check is the common
/// failure and needs no undo), then the store; a failed store write puts the previous
/// rules back.
#[allow(clippy::too_many_arguments)]
pub fn switch_files(
    rules: &crate::persistence::PolicyFile,
    rules_revision: Option<&str>,
    file: &ProfileFile,
    store_revision: Option<&str>,
    store: &Profiles,
    current: &[RoutingPolicy],
    target: &str,
) -> std::result::Result<Switched, SwitchError> {
    let (next, policies) = store
        .switch(target, current)
        .map_err(SwitchError::Refused)?;
    let written = rules
        .save(&policies, rules_revision)
        .map_err(SwitchError::Refused)?;
    match file.save(&next, store_revision) {
        Ok(store_revision) => Ok(Switched {
            store: next,
            store_revision,
            policies,
            rules_revision: written,
        }),
        Err(error) => match rules.save(current, Some(&written)) {
            Ok(rules_revision) => Err(SwitchError::Restored {
                error,
                rules_revision,
            }),
            Err(restore) => Err(SwitchError::Inconsistent {
                error,
                restore,
                policies,
                rules_revision: written,
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RouteMode, persistence::PolicyFile};

    #[test]
    fn switch_on_disk_writes_both_files_and_undoes_the_rules_when_the_store_fails() {
        let dir = tempfile::tempdir().unwrap();
        let rules = PolicyFile::new(dir.path().join("netlane-rules.json"));
        let file = ProfileFile::new(dir.path().join(PROFILES_FILE));
        let base = vec![rule("C:\\a.exe", RouteMode::WiFi)];
        let rules_revision = rules.save(&base, None).unwrap();
        let store = Profiles::implicit(&base).create("Gaming", &base).unwrap();
        let store_revision = file.save(&store, None).unwrap();
        // Gaming is active; the user saved an edit to it.
        let gaming_edit = vec![rule("C:\\a.exe", RouteMode::Ethernet)];
        let rules_revision = rules.save(&gaming_edit, Some(&rules_revision)).unwrap();

        let done = switch_files(
            &rules,
            Some(&rules_revision),
            &file,
            Some(&store_revision),
            &store,
            &gaming_edit,
            "Padrão",
        )
        .unwrap();
        assert_eq!(rules.load().unwrap().policies, base);
        let saved = file.load().unwrap().unwrap();
        assert_eq!(saved.profiles.active, "Padrão");
        assert_eq!(saved.profiles.profiles[1].policies, gaming_edit);
        assert_eq!(saved.revision, done.store_revision);

        // Store changed by someone else: the rules file is put back.
        std::fs::write(&file.path, serde_json::to_vec(&store).unwrap()).unwrap();
        let outcome = switch_files(
            &rules,
            Some(&done.rules_revision),
            &file,
            Some(&done.store_revision),
            &done.store,
            &done.policies,
            "Gaming",
        );
        let Err(SwitchError::Restored { rules_revision, .. }) = outcome else {
            panic!("restauração esperada: {outcome:?}");
        };
        let reread = rules.load().unwrap();
        assert_eq!(reread.policies, base);
        assert_eq!(reread.revision.as_deref(), Some(rules_revision.as_str()));

        // Stale rules revision: nothing is written at all.
        let before = std::fs::read(&file.path).unwrap();
        let outcome = switch_files(&rules, None, &file, None, &done.store, &base, "Gaming");
        assert!(matches!(outcome, Err(SwitchError::Refused(_))));
        assert_eq!(rules.load().unwrap().policies, base);
        assert_eq!(std::fs::read(&file.path).unwrap(), before);
    }

    fn rule(path: &str, mode: RouteMode) -> RoutingPolicy {
        let mut p = RoutingPolicy::new(path);
        p.route_mode = mode;
        p
    }

    #[test]
    fn switching_keeps_each_profile_rules_and_returns_the_target_set() {
        let base = [rule("C:\\Steam\\steam.exe", RouteMode::Automatic)];
        // The store may hold an older copy of the active profile: the saved rules win.
        let start = Profiles::implicit(&[]);
        let with_gaming = start.create("Gaming", &base).unwrap();
        assert_eq!(with_gaming.active, "Gaming");
        assert_eq!(with_gaming.profiles[0].policies, base);
        assert_eq!(with_gaming.profiles[1].policies, base);

        // Edits saved in Gaming are carried into its slot when switching away.
        let edited = [
            rule("C:\\Steam\\steam.exe", RouteMode::Ethernet),
            rule("C:\\Discord\\discord.exe", RouteMode::WiFi),
        ];
        let (back, rules) = with_gaming.switch("padrão", &edited).unwrap();
        assert_eq!(back.active, DEFAULT_PROFILE);
        assert_eq!(rules, base);
        let (again, rules) = back.switch("Gaming", &base).unwrap();
        assert_eq!(again.active, "Gaming");
        assert_eq!(rules, edited);
    }

    #[test]
    fn names_are_unique_bounded_and_the_active_profile_cannot_be_deleted() {
        let start = Profiles::implicit(&[]);
        assert!(start.create(" padrão ", &[]).is_err());
        assert!(start.create("   ", &[]).is_err());
        assert!(
            start
                .create(&"x".repeat(MAX_PROFILE_NAME + 1), &[])
                .is_err()
        );
        assert!(start.create("a\nb", &[]).is_err());
        let two = start.create("Trabalho", &[]).unwrap();
        assert!(two.delete("Trabalho").is_err());
        let one = two.delete("Padrão").unwrap();
        assert_eq!(one.names(), ["Trabalho"]);
        assert!(one.switch("Padrão", &[]).is_err());
        let mut full = Profiles::implicit(&[]);
        for i in 1..MAX_PROFILES {
            full = full.create(&format!("P{i}"), &[]).unwrap();
        }
        assert!(full.create("Mais um", &[]).is_err());
    }

    #[test]
    fn file_roundtrips_with_revision_control_and_refuses_invalid_content() {
        let dir = tempfile::tempdir().unwrap();
        let file = ProfileFile::new(dir.path().join(PROFILES_FILE));
        assert!(file.load().unwrap().is_none());
        let profiles = Profiles::implicit(&[rule("C:\\a.exe", RouteMode::WiFi)])
            .create("Gaming", &[])
            .unwrap();
        let revision = file.save(&profiles, None).unwrap();
        let loaded = file.load().unwrap().unwrap();
        assert_eq!(loaded.profiles, profiles);
        assert_eq!(loaded.revision, revision);
        // A stale revision never overwrites a newer file.
        assert!(file.save(&profiles, None).is_err());
        file.save(&profiles, Some(&revision)).unwrap();

        for invalid in [
            r#"{"active":"X","profiles":[{"name":"Y","policies":[]}]}"#,
            r#"{"active":"A","profiles":[{"name":"A","policies":[]},{"name":"a","policies":[]}]}"#,
            r#"{"active":"A","profiles":[{"name":"A","policies":[{"applicationId":"x","routeMode":99}]}]}"#,
            r#"{"active":"A","profiles":[]}"#,
        ] {
            std::fs::write(&file.path, invalid).unwrap();
            assert!(file.load().is_err(), "{invalid}");
            assert_eq!(std::fs::read_to_string(&file.path).unwrap(), invalid);
        }
    }
}
