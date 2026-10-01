use crate::cycle::{Decision, Sample};
use gateway_plugin_sdk::call::key_budgets::KeyBudget;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    pub at: i64,
    pub reason: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pending {
    Changed {
        windows: Vec<gateway_plugin_sdk::call::data::QuotaWindowFacts>,
    },
    Confirm {
        sample: Sample,
        reason: Decision,
    },
    Ready {
        sample: Sample,
        reason: String,
    },
    Unknown {
        sample: Sample,
        reason: String,
        before: KeyBudget,
        error: Option<String>,
    },
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Link {
    pub key_id: String,
    pub account_id: String,
    pub baseline: Sample,
    pub paused: bool,
    pub pending: Option<Pending>,
    pub events: VecDeque<Event>,
    #[serde(default)]
    pub retry_at: i64,
}
impl Link {
    pub fn event(&mut self, at: i64, reason: impl Into<String>) {
        self.events.push_back(Event {
            at,
            reason: reason.into(),
        });
        while self.events.len() > 10 {
            self.events.pop_front();
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Account {
    pub sample: Option<Sample>,
    pub next_check: i64,
    pub failure_since: Option<i64>,
    pub failures: u32,
    pub error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct State {
    pub early_auto: bool,
    pub links: BTreeMap<String, Link>,
    pub accounts: BTreeMap<String, Account>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            early_auto: true,
            links: BTreeMap::new(),
            accounts: BTreeMap::new(),
        }
    }
}
