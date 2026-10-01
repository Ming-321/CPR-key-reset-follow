use gateway_plugin_sdk::{
    PluginFault,
    call::{
        host::{AuthListRequest, AuthListResult, KeyListRequest},
        management::*,
    },
    client::{Empty, PluginBuilder, PluginSession, SessionConfig, TypedCall, TypedReply, methods},
};
use key_reset_follow::{
    engine::Engine,
    host::{Host, SdkHost, fault},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    action: String,
    key_id: Option<String>,
    account_id: Option<String>,
    version: Option<u64>,
    enabled: Option<bool>,
    display_timezone: Option<key_reset_follow::model::DisplayTimezone>,
    #[serde(default)]
    acknowledge: bool,
}
fn registration() -> ManagementRegistration {
    ManagementRegistration {
        routes: vec![
            ManagementRoute {
                method: "GET".into(),
                path: "state".into(),
                request_content_types: vec![],
                response_content_types: vec!["application/json".into()],
            },
            ManagementRoute {
                method: "POST".into(),
                path: "action".into(),
                request_content_types: vec!["application/json".into()],
                response_content_types: vec!["application/json".into()],
            },
        ],
        resources: ["web/index.html", "web/app.js", "web/app.css"]
            .into_iter()
            .map(|path| ManagementResource {
                path: path.into(),
                public: false,
            })
            .collect(),
        pages: vec![ManagementPage {
            id: "follow".into(),
            title: "Key 周额度跟随账号重置".into(),
            description: Some("确认账号新周期后清零关联 Key 周预算".into()),
            entry: "web/index.html".into(),
            icon: None,
        }],
        callbacks: vec![],
    }
}
async fn handle(call: TypedCall<ManagementRequest>) -> Result<Value, PluginFault> {
    let host = SdkHost(call.host);
    let mut engine = Engine::load(&host).await?;
    if call.request.method == "POST" {
        let cmd: Command =
            serde_json::from_slice(&call.payload).map_err(|_| fault("请求格式不正确"))?;
        if cmd.action != "preview" && cmd.version != engine.version {
            return Err(fault("页面状态已变化，请刷新后重试"));
        }
        let key = cmd.key_id.as_deref().unwrap_or("");
        match cmd.action.as_str() {
            "preview" => {
                return Ok(
                    json!({"sample": engine.fresh(cmd.account_id.as_deref().ok_or_else(|| fault("请选择账号"))?).await?}),
                );
            }
            "add" => {
                engine
                    .add(
                        key,
                        cmd.account_id
                            .as_deref()
                            .ok_or_else(|| fault("请选择账号"))?,
                        now(),
                    )
                    .await?
            }
            "settings" => {
                engine.state.early_auto = cmd.enabled.ok_or_else(|| fault("缺少开关值"))?;
                if let Some(timezone) = cmd.display_timezone {
                    engine.state.display_timezone = timezone;
                }
                engine.save().await?;
            }
            "check" => {
                let account = engine
                    .state
                    .links
                    .get(key)
                    .ok_or_else(|| fault("关联不存在"))?
                    .account_id
                    .clone();
                engine.check(&account, now()).await?;
            }
            action => engine.action(key, action, cmd.acknowledge, now()).await?,
        }
        return Ok(json!({"ok": true}));
    }
    let mut keys = vec![];
    let mut cursor = None;
    loop {
        let page = host
            .0
            .list_keys(KeyListRequest { cursor, limit: 200 })
            .await?;
        keys.extend(page.keys);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let mut accounts = vec![];
    let mut cursor = None;
    loop {
        let reply = host
            .0
            .call(
                "host.auth.list",
                json!({}),
                serde_json::to_vec(&AuthListRequest {
                    provider_id: None,
                    cursor,
                    limit: 200,
                })
                .map_err(|_| fault("账号查询编码失败"))?,
            )
            .await
            .map_err(gateway_plugin_sdk::client::SessionError::into_plugin_fault)?;
        let page: AuthListResult =
            serde_json::from_slice(&reply.payload).map_err(|_| fault("账号目录解析失败"))?;
        accounts.extend(page.accounts.into_iter().map(|a| {
            json!({
                "account_id": a.account_id, "name": a.name, "email": a.email,
                "provider_id": a.provider_id, "authentication_kind": a.authentication_kind,
            })
        }));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let mut budgets = serde_json::Map::new();
    for key in &keys {
        if let Ok(budget) = host.budget(&key.id).await {
            budgets.insert(key.id.clone(), json!(budget));
        }
    }
    Ok(
        json!({"version": engine.version, "state": engine.state, "keys": keys, "accounts": accounts, "budgets": budgets}),
    )
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lock = Arc::new(Mutex::new(()));
    let management_lock = lock.clone();
    let plugin = PluginBuilder::from_json(include_bytes!("../plugin.json"))?
        .management(registration(), move |call| {
            let lock = management_lock.clone();
            async move {
                let _guard = lock.lock().await;
                let (status, body) = match handle(call).await {
                    Ok(v) => (200, v),
                    Err(e) => (400, json!({"error": e.message})),
                };
                Ok(TypedReply::new(ManagementResponse {
                    status,
                    content_type: "application/json".into(),
                    headers: vec![],
                })
                .with_payload(serde_json::to_vec(&body).unwrap()))
            }
        })?
        .on(methods::RECONCILE, move |call| {
            let lock = lock.clone();
            async move {
                let _guard = lock.lock().await;
                let host = SdkHost(call.host);
                let mut engine = Engine::load(&host).await?;
                engine.drain(now()).await?;
                let mut due: Vec<_> = engine
                    .state
                    .accounts
                    .iter()
                    .filter(|(id, a)| {
                        a.next_check <= now()
                            && engine
                                .state
                                .links
                                .values()
                                .any(|l| &l.account_id == *id && !l.paused)
                    })
                    .map(|(id, a)| (a.next_check, id.clone()))
                    .collect();
                due.sort();
                for (_, id) in due.into_iter().take(2) {
                    engine.check(&id, now()).await?;
                }
                Ok(TypedReply::new(Empty {}))
            }
        })?
        .build()?;
    PluginSession::accept(
        tokio::io::stdin(),
        tokio::io::stdout(),
        SessionConfig::default(),
    )
    .await?
    .run(plugin)
    .await?;
    Ok(())
}
