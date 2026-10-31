# v0.1.5

适配 CPR v3.36.0，支持 **CPR >=3.36.0、<3.37.0**。SDK 和打包 CLI 固定到官方提交 `5ae38dc0c55c192815ef605dde6c05fd767a2c28`，使用 RPC v7、management v2、maintenance v1。

业务逻辑、页面源码与状态 Schema 保持不变。通过原插件的更新或版本切换升级，保留原有设置与私有状态；不要卸载重装。旧 CPR 请使用先前发布的兼容版本。

提供 Linux x86_64、ARM64 安装包及 SHA-256 校验文件。

验证范围：

- macOS ARM64 / Rust 1.99.1：格式检查、严格 Clippy 和 19 项逻辑测试通过
- Node 24：前端 lint、测试、类型检查及生产构建通过；pnpm 12.6.0 的 macOS 原生程序无法启动，使用 pnpm 11.25.0 冻结锁文件构建，前端依赖与锁文件未修改
- 两个 Linux 架构使用 cargo-zigbuild 交叉编译，glibc 目标基线为 2.31；使用官方 v3.36.0 CLI 打包并核对清单、架构和文件摘要
- 未执行 Linux 二进制运行测试，未完成 CPR v3.36.0 宿主安装、管理页面及升级状态保留联调，这些场景由使用者在隔离环境测试

升级后按 [宿主测试清单](https://github.com/Ming-321/CPR-key-reset-follow/blob/main/docs/local-release.md#用户宿主测试清单) 验证。
