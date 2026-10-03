use gateway_plugin_sdk::call::data::QuotaFacts;
use serde::{Deserialize, Serialize};

pub const WEEK: i64 = 604_800_000;
pub const TOLERANCE: i64 = 120_000;
pub const SETTLE_INTERVAL: i64 = 300_000;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub observed: i64,
    pub reset: i64,
    pub used: Option<f64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Same,
    Normal,
    Early,
    Unexplained,
    Stale,
}

pub fn sample(facts: &QuotaFacts) -> Result<Sample, String> {
    let candidates: Vec<_> = facts
        .windows
        .iter()
        .filter(|w| {
            w.key.starts_with("codex:")
                && (w.window_seconds == Some(604800) || w.key.starts_with("codex:604800s"))
        })
        .collect();
    if candidates.len() != 1 {
        return Err("未找到唯一的 Codex 主额度周窗口，请在宿主刷新账号并检查套餐".into());
    }
    let w = candidates[0];
    if w.key != "codex:604800s" || w.window_seconds != Some(604800) {
        return Err("窗口标识或时长不符合 Codex 主额度周窗口，请检查账号套餐".into());
    }
    Ok(Sample {
        observed: facts.observed_at_ms.ok_or("缺少观测时间")?,
        reset: w.reset_at_ms.ok_or("缺少上游重置时间")?,
        used: w.used_percent,
    })
}

pub fn classify(reference: &Sample, next: &Sample) -> Decision {
    if next.observed <= reference.observed {
        return Decision::Stale;
    }
    let delta = i128::from(next.reset) - i128::from(reference.reset);
    if delta.abs() <= i128::from(TOLERANCE) {
        return Decision::Same;
    }
    let start = i128::from(next.reset) - i128::from(WEEK);
    if delta > i128::from(TOLERANCE)
        && start >= i128::from(reference.observed.min(reference.reset)) - i128::from(TOLERANCE)
        && start <= i128::from(next.observed) + i128::from(TOLERANCE)
    {
        if start >= i128::from(reference.reset) - i128::from(TOLERANCE) {
            Decision::Normal
        } else {
            Decision::Early
        }
    } else {
        Decision::Unexplained
    }
}

pub fn floating(sample: &Sample) -> bool {
    (i128::from(sample.reset) - i128::from(sample.observed) - i128::from(WEEK)).abs()
        <= i128::from(TOLERANCE)
}
