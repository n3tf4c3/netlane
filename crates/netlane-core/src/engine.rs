use crate::{RouteMode, planning::PlannedRule};
use anyhow::Result;

pub trait PolicySession {
    type Key;
    fn begin(&mut self) -> Result<()>;
    fn add(&mut self, rule: &PlannedRule) -> Result<Vec<Self::Key>>;
    fn delete(&mut self, key: &Self::Key) -> Result<()>;
    fn commit(&mut self) -> Result<()>;
    fn abort(&mut self) -> Result<()>;
    fn close(&mut self) -> Result<()>;
}
pub struct RoutingEngine<S: PolicySession> {
    session: S,
    previous: Vec<PlannedRule>,
    keys: Vec<S::Key>,
    poisoned: bool,
}
impl<S: PolicySession> RoutingEngine<S> {
    pub fn new(session: S) -> Self {
        Self {
            session,
            previous: Vec::new(),
            keys: Vec::new(),
            poisoned: false,
        }
    }
    pub fn apply(&mut self, rules: &[PlannedRule]) -> Result<()> {
        anyhow::ensure!(!self.poisoned, "Sessão encerrada após falha de rollback.");
        if rules == self.previous {
            return Ok(());
        }
        self.session.begin()?;
        let result = (|| {
            for key in &self.keys {
                self.session.delete(key)?;
            }
            let mut keys = Vec::new();
            for rule in rules.iter().filter(|r| r.mode != RouteMode::Automatic) {
                keys.extend(self.session.add(rule)?);
            }
            self.session.commit()?;
            Ok(keys)
        })();
        match result {
            Ok(keys) => {
                self.previous = rules.to_vec();
                self.keys = keys;
                Ok(())
            }
            Err(error) => {
                if self.session.abort().is_err() {
                    self.poisoned = true;
                    let _ = self.session.close();
                    self.keys.clear();
                    self.previous.clear();
                }
                Err(error)
            }
        }
    }
    /// A failed apply that was rolled back keeps the previous policies; only a failed
    /// rollback (or close) ends the session.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    pub fn close(&mut self) -> Result<()> {
        self.poisoned = true;
        self.previous.clear();
        self.keys.clear();
        self.session.close()
    }
}
impl<S: PolicySession> Drop for RoutingEngine<S> {
    fn drop(&mut self) {
        let _ = self.session.close();
    }
}
