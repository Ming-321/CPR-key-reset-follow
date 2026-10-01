# 贡献指南

## 开发依据

宿主与 SDK 固定官方 v3.18.3 提交 `9c1067676495623111497111b7cb136544fe1828`，UI 使用官方 v0.3.0 发行包
先阅读 [设计合同](docs/design.md)，周期判定与重置语义属于数据安全边界

使用 Rust 1.97.0、Node.js 24 和 pnpm 12.6.0

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend lint
pnpm --dir frontend build
```

联调必须在专用官方宿主及专用 PostgreSQL、Redis 中进行，使用合成账号与额度响应
通过真实 SDK、插件进程、安装器和管理页面验证，不以直接改库代替业务链路
响应丢失和进程退出可在传输边界定向注入，说明直接验证与推断范围

## 提交与 PR

功能分支向 main 提 PR，检查通过后 squash 合并
提交使用英文 Conventional Commits，文档使用简体中文
PR 说明问题、行为、实际验证和未验证部分；页面变动附当前实现的合成数据截图
使用 AI 时披露工具和实际模型型号，未知时如实注明并保持 Draft
不得上传真实账号、令牌、生产地址和生产截图

## 打包与发布

先构建插件二进制与页面，再从固定宿主提交构建 `codex-proxy-plugin-cli`

```sh
cargo build --release --locked
cpr-plugin package --manifest plugin.json --binary target/release/key-reset-follow \
  --target x86_64-unknown-linux-gnu --resource-map web=frontend/dist --output-dir dist
```

target 必须匹配实际编译平台，不通过修改平台参数伪装交叉编译
版本修改经 PR 合并，`v<version>` 标签与 plugin.json 逐字一致，只标记已验证提交
CI 发布 Linux x86_64 的 tar.gz 与 SHA-256 校验文件，带后缀的版本标为预发行
已发布标签不移动，附件不覆盖；修正内容发布新版本
从预发行升级稳定版使用单独版本 PR，生产安装和稳定版发布须有对应用户授权
下载公开附件并核对摘要后才安装，安装后检查原有关联与状态保留
