use gateway_plugin_sdk::{
    PluginFault,
    call::{
        data::{QuotaFacts, QuotaWindowFacts},
        key_budgets::KeyBudget,
    },
};
use key_reset_follow::{
    cycle::WEEK,
    engine::Engine,
    host::{Host, fault},
    model::{Pending, State},
};
use std::sync::Mutex;
#[derive(Default)]
struct Storage {
    state: State,
    version: u64,
    resets: Vec<String>,
    refreshes: usize,
    time: i64,
    reset_at: i64,
    fail_save_at: Option<u64>,
    lost_response: Option<String>,
    hang_reset: bool,
    missing_window: bool,
    changed_window: bool,
    fail_budget: Option<String>,
}
struct Fake(Mutex<Storage>);
impl Fake {
    fn new() -> Self {
        Self(Mutex::new(Storage {
            time: 1000000,
            reset_at: WEEK,
            ..Default::default()
        }))
    }
}
impl Host for Fake {
    async fn load(&self) -> Result<(State, Option<u64>), PluginFault> {
        let s = self.0.lock().unwrap();
        Ok((s.state.clone(), (s.version > 0).then_some(s.version)))
    }
    async fn save(&self, state: &State, version: Option<u64>) -> Result<u64, PluginFault> {
        let mut s = self.0.lock().unwrap();
        if version.unwrap_or(0) != s.version || s.fail_save_at == Some(s.version + 1) {
            return Err(fault("injected state failure"));
        }
        s.version += 1;
        s.state = state.clone();
        Ok(s.version)
    }
    async fn refresh(&self, account: &str) -> Result<QuotaFacts, PluginFault> {
        let mut s = self.0.lock().unwrap();
        s.refreshes += 1;
        Ok(QuotaFacts {
            schema_version: 1,
            account_id: account.into(),
            observed_at_ms: Some(s.time),
            windows: if s.missing_window {
                vec![]
            } else {
                vec![QuotaWindowFacts {
                    key: if s.changed_window {
                        "codex:604800s:secondary_window"
                    } else {
                        "codex:604800s"
                    }
                    .into(),
                    window_seconds: Some(604800),
                    used_percent: Some(10.0),
                    reset_at_ms: Some(s.reset_at),
                }]
            },
        })
    }
    async fn budget(&self, key: &str) -> Result<KeyBudget, PluginFault> {
        if self.0.lock().unwrap().fail_budget.as_deref() == Some(key) {
            return Err(fault("budget unavailable"));
        }
        Ok(KeyBudget {
            client_key_id: key.into(),
            daily_limit_usd: "0".into(),
            weekly_limit_usd: "60".into(),
            daily_used_usd: "0".into(),
            weekly_used_usd: "12".into(),
            daily_resets_at_ms: None,
            weekly_resets_at_ms: Some(WEEK),
        })
    }
    async fn reset(&self, key: &str) -> Result<(), PluginFault> {
        let hang = self.0.lock().unwrap().hang_reset;
        if hang {
            std::future::pending::<()>().await;
        }
        let mut s = self.0.lock().unwrap();
        s.resets.push(key.into());
        if s.lost_response.as_deref() == Some(key) {
            Err(fault("response lost after commit"))
        } else {
            Ok(())
        }
    }
}
async fn setup(h: &Fake) {
    let mut e = Engine::load(h).await.unwrap();
    e.add("key-a", "account-a", 1000000).await.unwrap();
    e.add("key-b", "account-a", 1000000).await.unwrap();
}
fn next(h: &Fake) {
    let mut s = h.0.lock().unwrap();
    s.time = WEEK + 1000;
    s.reset_at = WEEK * 2;
}
#[tokio::test]
async fn shared_refresh_and_partial_unknown_do_not_repeat_success() {
    let h = Fake::new();
    setup(&h).await;
    assert!(h.0.lock().unwrap().resets.is_empty());
    next(&h);
    h.0.lock().unwrap().lost_response = Some("key-b".into());
    let before = h.0.lock().unwrap().refreshes;
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().refreshes, before + 1);
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-a", "key-b"]);
    Engine::load(&h)
        .await
        .unwrap()
        .drain(WEEK + 2000)
        .await
        .unwrap();
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 2000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets.len(), 2);
    assert!(matches!(
        h.0.lock().unwrap().state.links["key-b"].pending,
        Some(Pending::Unknown { .. })
    ));
}
#[tokio::test]
async fn completion_write_failure_cannot_repeat_reset() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    // check 保存待处理、保存意图后，故障发生于完成落盘。
    h.0.lock().unwrap().fail_save_at = Some(5);
    assert!(
        Engine::load(&h)
            .await
            .unwrap()
            .check("account-a", WEEK + 1000)
            .await
            .is_err()
    );
    h.0.lock().unwrap().fail_save_at = None;
    Engine::load(&h)
        .await
        .unwrap()
        .drain(WEEK + 2000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-a", "key-b"]);
    assert!(matches!(
        h.0.lock().unwrap().state.links["key-a"].pending,
        Some(Pending::Unknown { .. })
    ));
}
#[tokio::test]
async fn failed_intent_write_does_not_call_reset() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().fail_save_at = Some(4);
    assert!(
        Engine::load(&h)
            .await
            .unwrap()
            .check("account-a", WEEK + 1000)
            .await
            .is_err()
    );
    assert!(h.0.lock().unwrap().resets.is_empty());
}
#[tokio::test]
async fn early_switch_and_manual_skip_preserve_usage() {
    let h = Fake::new();
    setup(&h).await;
    {
        let mut s = h.0.lock().unwrap();
        s.state.early_auto = false;
        s.time = 2000000;
        s.reset_at = WEEK + 1500000;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", 2000000)
        .await
        .unwrap();
    assert!(h.0.lock().unwrap().resets.is_empty());
    assert!(matches!(
        h.0.lock().unwrap().state.links["key-a"].pending,
        Some(Pending::Confirm { .. })
    ));
    Engine::load(&h)
        .await
        .unwrap()
        .action("key-a", "skip", false, 2000000)
        .await
        .unwrap();
    assert!(h.0.lock().unwrap().resets.is_empty());
    assert_eq!(
        h.0.lock().unwrap().state.links["key-a"].baseline.reset,
        WEEK + 1500000
    );
}
#[tokio::test]
async fn concurrent_stale_state_cannot_claim_reset() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    let mut old = Engine::load(&h).await.unwrap();
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    assert!(old.check("account-a", WEEK + 1000).await.is_err());
    assert_eq!(h.0.lock().unwrap().resets.len(), 2);
}

#[tokio::test]
async fn termination_after_durable_intent_never_replays() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().hang_reset = true;
    let mut e = Engine::load(&h).await.unwrap();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            e.check("account-a", WEEK + 1000)
        )
        .await
        .is_err()
    );
    drop(e);
    h.0.lock().unwrap().hang_reset = false;
    Engine::load(&h)
        .await
        .unwrap()
        .drain(WEEK + 2000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b"]);
    assert!(matches!(
        h.0.lock().unwrap().state.links["key-a"].pending,
        Some(Pending::Unknown { .. })
    ));
}
#[tokio::test]
async fn pause_resume_and_accounts_are_independent() {
    let h = Fake::new();
    setup(&h).await;
    Engine::load(&h)
        .await
        .unwrap()
        .add("key-c", "account-b", 1000000)
        .await
        .unwrap();
    Engine::load(&h)
        .await
        .unwrap()
        .action("key-a", "pause", false, 1000000)
        .await
        .unwrap();
    next(&h);
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b"]);
    Engine::load(&h)
        .await
        .unwrap()
        .action("key-a", "resume", false, WEEK + 2000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b"]);
    assert_eq!(
        h.0.lock().unwrap().state.links["key-c"].baseline.reset,
        WEEK
    );
}
#[tokio::test]
async fn missing_samples_keep_baseline_and_changed_identity_requires_confirmation() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().missing_window = true;
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    assert_eq!(
        h.0.lock().unwrap().state.links["key-a"].baseline.reset,
        WEEK
    );
    assert!(h.0.lock().unwrap().resets.is_empty());
    {
        let mut s = h.0.lock().unwrap();
        s.missing_window = false;
        s.changed_window = true;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 2000)
        .await
        .unwrap();
    assert!(matches!(
        h.0.lock().unwrap().state.links["key-a"].pending,
        Some(Pending::Changed { .. })
    ));
    assert!(h.0.lock().unwrap().resets.is_empty());
}
#[tokio::test]
async fn budget_failure_leaves_other_keys_progressing_and_retries_only_unclaimed_key() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().fail_budget = Some("key-a".into());
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b"]);
    h.0.lock().unwrap().fail_budget = None;
    Engine::load(&h)
        .await
        .unwrap()
        .drain(WEEK + 301000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b", "key-a"]);
}
#[tokio::test]
async fn early_auto_is_enabled_by_default_and_unknown_retry_requires_acknowledgement() {
    let h = Fake::new();
    setup(&h).await;
    {
        let mut s = h.0.lock().unwrap();
        s.time = 2000000;
        s.reset_at = WEEK + 1500000;
        s.lost_response = Some("key-a".into());
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", 2000000)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets.len(), 2);
    assert!(
        Engine::load(&h)
            .await
            .unwrap()
            .action("key-a", "retry", false, 2000000)
            .await
            .is_err()
    );
    assert_eq!(h.0.lock().unwrap().resets.len(), 2);
}

#[tokio::test]
async fn prolonged_outage_discards_queued_cycle_before_any_reset() {
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().fail_budget = Some("key-a".into());
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    h.0.lock().unwrap().fail_budget = None;
    Engine::load(&h)
        .await
        .unwrap()
        .drain(WEEK * 3)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b"]);
    assert!(h.0.lock().unwrap().state.links["key-a"].pending.is_none());
    {
        let mut s = h.0.lock().unwrap();
        s.time = WEEK * 3;
        s.reset_at = WEEK * 4;
    }
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK * 3)
        .await
        .unwrap();
    assert_eq!(h.0.lock().unwrap().resets, vec!["key-b", "key-a", "key-b"]);
    assert_eq!(
        h.0.lock().unwrap().state.links["key-a"].baseline.reset,
        WEEK * 4
    );
}

#[tokio::test]
async fn beta_state_and_pending_reset_survive_display_settings() {
    use key_reset_follow::model::DisplayTimezone;
    let h = Fake::new();
    setup(&h).await;
    next(&h);
    h.0.lock().unwrap().lost_response = Some("key-a".into());
    Engine::load(&h)
        .await
        .unwrap()
        .check("account-a", WEEK + 1000)
        .await
        .unwrap();
    let mut old = serde_json::to_value(&h.0.lock().unwrap().state).unwrap();
    old.as_object_mut().unwrap().remove("display_timezone");
    let recovered: State = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(recovered.display_timezone, DisplayTimezone::Browser);
    h.0.lock().unwrap().state = recovered;
    let mut e = Engine::load(&h).await.unwrap();
    e.state.display_timezone = DisplayTimezone::Shanghai;
    e.state.early_auto = false;
    e.save().await.unwrap();
    let after = serde_json::to_value(&h.0.lock().unwrap().state).unwrap();
    assert_eq!(after["links"], old["links"]);
    assert_eq!(after["accounts"], old["accounts"]);
    assert_eq!(h.0.lock().unwrap().resets.len(), 2);
    assert!(serde_json::from_value::<DisplayTimezone>(serde_json::json!("unsupported")).is_err());
}
