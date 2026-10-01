use crate::{
    cycle::{self, Decision, Sample},
    host::{Host, fault},
    model::{Link, Pending, State},
};
use gateway_plugin_sdk::PluginFault;

pub struct Engine<'a, H> {
    pub host: &'a H,
    pub state: State,
    pub version: Option<u64>,
}
impl<'a, H: Host> Engine<'a, H> {
    pub async fn load(host: &'a H) -> Result<Self, PluginFault> {
        let (state, version) = host.load().await?;
        Ok(Self {
            host,
            state,
            version,
        })
    }
    pub async fn save(&mut self) -> Result<(), PluginFault> {
        self.version = Some(self.host.save(&self.state, self.version).await?);
        Ok(())
    }
    pub async fn fresh(&self, account: &str) -> Result<Sample, PluginFault> {
        let facts = self.host.refresh(account).await?;
        if facts.account_id != account {
            return Err(fault("上游返回了其他账号的样本"));
        }
        cycle::sample(&facts).map_err(fault)
    }
    pub async fn add(&mut self, key: &str, account: &str, now: i64) -> Result<(), PluginFault> {
        if self.state.links.contains_key(key) {
            return Err(fault("该 Key 已有关联"));
        }
        self.host.budget(key).await?;
        let baseline = self.fresh(account).await?;
        self.state.accounts.entry(account.into()).or_default();
        self.state.links.insert(
            key.into(),
            Link {
                key_id: key.into(),
                account_id: account.into(),
                baseline,
                paused: false,
                pending: None,
                events: Default::default(),
                retry_at: 0,
            },
        );
        self.state
            .links
            .get_mut(key)
            .unwrap()
            .event(now, "建立基线，不清零");
        self.save().await
    }
    pub async fn check(&mut self, account_id: &str, now: i64) -> Result<(), PluginFault> {
        let facts = self.host.refresh(account_id).await;
        if let Ok(facts) = &facts {
            let changed = facts.windows.iter().any(|w| {
                w.key.starts_with("codex:604800s")
                    && (w.key != "codex:604800s" || w.window_seconds != Some(604800))
            });
            if facts.account_id == account_id && changed {
                for link in self
                    .state
                    .links
                    .values_mut()
                    .filter(|l| l.account_id == account_id && !l.paused && l.pending.is_none())
                {
                    if facts
                        .observed_at_ms
                        .is_some_and(|t| t > link.baseline.observed)
                    {
                        link.pending = Some(Pending::Changed {
                            windows: facts.windows.clone(),
                        });
                    }
                }
            }
        }
        let result = facts.and_then(|facts| {
            if facts.account_id != account_id {
                return Err(fault("账号样本不匹配"));
            }
            cycle::sample(&facts).map_err(fault)
        });
        let account = self.state.accounts.entry(account_id.into()).or_default();
        let sample = match result {
            Ok(s)
                if account
                    .sample
                    .as_ref()
                    .is_none_or(|prev| s.observed > prev.observed) =>
            {
                s
            }
            result => {
                account.failure_since.get_or_insert(now);
                account.failures = account.failures.saturating_add(1);
                account.next_check =
                    now + (30_000_i64.saturating_mul(1 << account.failures.min(6))).min(900_000);
                // 不透传可能包含代理地址或上游响应正文的错误详情。
                account.error = Some(match result {
                    Ok(_) => "上游仍返回旧样本".into(),
                    Err(e) => format!("无法刷新账号额度（{:?}）", e.code),
                });
                return self.save().await;
            }
        };
        account.sample = Some(sample.clone());
        account.next_check = now + 300_000;
        account.failure_since = None;
        account.failures = 0;
        account.error = None;
        for link in self
            .state
            .links
            .values_mut()
            .filter(|l| l.account_id == account_id && !l.paused && l.pending.is_none())
        {
            let decision = cycle::classify(&link.baseline, &sample);
            match decision {
                Decision::Same => link.baseline.observed = sample.observed,
                Decision::Normal | Decision::Early
                    if decision == Decision::Normal || self.state.early_auto =>
                {
                    link.pending = Some(Pending::Ready {
                        sample: sample.clone(),
                        reason: if decision == Decision::Normal {
                            "正常换周"
                        } else {
                            "提前重置"
                        }
                        .into(),
                    });
                }
                Decision::Early | Decision::Unexplained => {
                    link.pending = Some(Pending::Confirm {
                        sample: sample.clone(),
                        reason: decision,
                    })
                }
                Decision::Stale | Decision::Normal => {}
            }
        }
        self.save().await?;
        self.drain(now).await
    }
    pub async fn drain(&mut self, now: i64) -> Result<(), PluginFault> {
        let keys: Vec<_> = self
            .state
            .links
            .iter()
            .filter(|(_, l)| {
                !l.paused && l.retry_at <= now && matches!(l.pending, Some(Pending::Ready { .. }))
            })
            .take(8)
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys {
            self.execute(&key, now).await?;
        }
        Ok(())
    }
    pub async fn execute(&mut self, key: &str, now: i64) -> Result<(), PluginFault> {
        let Some(Pending::Ready { sample, reason }) =
            self.state.links.get(key).and_then(|l| l.pending.clone())
        else {
            return Ok(());
        };
        let before = match self.host.budget(key).await {
            Ok(b) => b,
            Err(_) => {
                self.state.links.get_mut(key).unwrap().retry_at = now + 300_000;
                self.state
                    .links
                    .get_mut(key)
                    .unwrap()
                    .event(now, "读取预算失败，尚未调用清零");
                return self.save().await;
            }
        };
        // 未完成意图就是未知结果，重启无需猜测调用是否曾发送。
        self.state.links.get_mut(key).unwrap().pending = Some(Pending::Unknown {
            sample: sample.clone(),
            reason: reason.clone(),
            before,
            error: None,
        });
        self.save().await?;
        let result = self.host.reset(key).await;
        let link = self.state.links.get_mut(key).unwrap();
        match result {
            Ok(()) => {
                link.baseline = sample;
                link.pending = None;
                link.event(now, reason);
            }
            Err(e) => {
                if let Some(Pending::Unknown { error, .. }) = &mut link.pending {
                    *error = Some(format!("{:?}", e.code));
                }
                link.event(now, "结果未知，已停止自动重试");
            }
        }
        self.save().await
    }
    pub async fn action(
        &mut self,
        key: &str,
        action: &str,
        acknowledge: bool,
        now: i64,
    ) -> Result<(), PluginFault> {
        let link = self
            .state
            .links
            .get(key)
            .cloned()
            .ok_or_else(|| fault("关联不存在，请刷新页面"))?;
        match action {
            "pause" => {
                self.state.links.get_mut(key).unwrap().paused = true;
            }
            "delete" if acknowledge => {
                self.state.links.remove(key);
                self.state
                    .accounts
                    .retain(|id, _| self.state.links.values().any(|l| &l.account_id == id));
            }
            "resume" | "skip" => {
                if action == "resume" && !link.paused {
                    return Err(fault("关联未暂停"));
                }
                if action == "skip" && link.pending.is_none() {
                    return Err(fault("没有待处理的变化"));
                }
                let fresh = self.fresh(&link.account_id).await?;
                let l = self.state.links.get_mut(key).unwrap();
                l.baseline = fresh;
                l.pending = None;
                l.paused = false;
                l.event(now, "保留现有用量并重新建立基线");
            }
            "confirm" | "retry" => {
                if link.paused {
                    return Err(fault("关联已暂停"));
                }
                let fresh = self.fresh(&link.account_id).await?;
                let expected = match &link.pending {
                    Some(Pending::Changed { .. }) if action == "confirm" => &fresh,
                    Some(Pending::Confirm { sample, .. }) if action == "confirm" => sample,
                    Some(Pending::Unknown { sample, .. }) if action == "retry" && acknowledge => {
                        sample
                    }
                    _ => return Err(fault("状态已变化或尚未确认不可逆后果，请刷新页面")),
                };
                if (i128::from(fresh.reset) - i128::from(expected.reset)).abs()
                    > i128::from(cycle::TOLERANCE)
                {
                    return Err(fault("上游周期已变化，请先跳过并重新建立基线"));
                }
                self.state.links.get_mut(key).unwrap().pending = Some(Pending::Ready {
                    sample: fresh,
                    reason: if action == "retry" {
                        "人工再次清零"
                    } else {
                        "手动确认"
                    }
                    .into(),
                });
                self.save().await?;
                return self.execute(key, now).await;
            }
            _ => return Err(fault("无效操作或缺少删除确认")),
        }
        self.save().await
    }
}
