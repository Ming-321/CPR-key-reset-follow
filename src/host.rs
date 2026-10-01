use crate::model::State;
use gateway_plugin_sdk::{
    ErrorCode, PluginFault,
    call::{
        data::{QuotaFacts, QuotaFactsQuery},
        host::{StateGetRequest, StateGetResult, StatePutRequest, StatePutResult},
        key_budgets::{BudgetPeriod, GetKeyBudgetRequest, KeyBudget, ResetKeyBudgetRequest},
    },
    client::{HostClient, SessionError},
};
use std::future::Future;

pub fn fault(message: impl Into<String>) -> PluginFault {
    PluginFault::new(ErrorCode::Rejected, message)
}
pub trait Host: Sync {
    fn load(&self) -> impl Future<Output = Result<(State, Option<u64>), PluginFault>> + Send;
    fn save(
        &self,
        state: &State,
        version: Option<u64>,
    ) -> impl Future<Output = Result<u64, PluginFault>> + Send;
    fn refresh(
        &self,
        account: &str,
    ) -> impl Future<Output = Result<QuotaFacts, PluginFault>> + Send;
    fn budget(&self, key: &str) -> impl Future<Output = Result<KeyBudget, PluginFault>> + Send;
    fn reset(&self, key: &str) -> impl Future<Output = Result<(), PluginFault>> + Send;
}
pub struct SdkHost(pub HostClient);
impl Host for SdkHost {
    async fn load(&self) -> Result<(State, Option<u64>), PluginFault> {
        let reply = self
            .0
            .call(
                "host.state.get",
                serde_json::to_value(StateGetRequest {
                    namespace: "follow".into(),
                    key: "state".into(),
                })
                .map_err(|_| fault("状态查询编码失败"))?,
                vec![],
            )
            .await
            .map_err(SessionError::into_plugin_fault)?;
        let result: StateGetResult =
            serde_json::from_value(reply.result).map_err(|_| fault("状态响应解析失败"))?;
        match result.record {
            Some(record) => Ok((
                serde_json::from_value(record.value)
                    .map_err(|_| fault("状态格式不兼容，已停止处理"))?,
                Some(record.version),
            )),
            None => Ok((State::default(), None)),
        }
    }
    async fn save(&self, state: &State, version: Option<u64>) -> Result<u64, PluginFault> {
        let request = StatePutRequest {
            namespace: "follow".into(),
            key: "state".into(),
            value: serde_json::to_value(state).map_err(|_| fault("状态编码失败"))?,
            expected_version: version,
        };
        let reply = self
            .0
            .call(
                "host.state.put",
                serde_json::to_value(request).map_err(|_| fault("状态编码失败"))?,
                vec![],
            )
            .await
            .map_err(SessionError::into_plugin_fault)?;
        let result: StatePutResult =
            serde_json::from_value(reply.result).map_err(|_| fault("状态写入结果未知"))?;
        Ok(result.version)
    }
    async fn refresh(&self, account: &str) -> Result<QuotaFacts, PluginFault> {
        self.0
            .refresh_account_quota(QuotaFactsQuery {
                account_id: account.into(),
            })
            .await
    }
    async fn budget(&self, key: &str) -> Result<KeyBudget, PluginFault> {
        self.0
            .get_key_budget(GetKeyBudgetRequest {
                client_key_id: key.into(),
            })
            .await
    }
    async fn reset(&self, key: &str) -> Result<(), PluginFault> {
        self.0
            .reset_key_budget(ResetKeyBudgetRequest {
                client_key_id: key.into(),
                period: BudgetPeriod::Weekly,
            })
            .await
            .map(|_| ())
    }
}
