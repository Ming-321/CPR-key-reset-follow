# v0.1.13

适配 CPR v3.47.0，支持 **CPR >=3.47.0、<3.48.0**。SDK 和打包 CLI 固定到官方提交 `0e02a056b5a4fcb9fe204e741cbc20575041d7ba`，继续使用 RPC v7、management v2、maintenance v1。

通过原插件更新或版本切换升级，沿用现有设置、关联与私有状态。不要卸载重装；旧 CPR 请使用先前兼容版本。业务逻辑、状态 Schema 和页面不变，保留 `cp` 主题命名空间声明。

如果其他插件管理某个 Key 的显式周预算窗口，不要将该 Key 加入跟随。跟随清零会退出显式模式，Key 下次使用时按 CPR 原生规则开启周窗口。

提供 Linux ARM64、x86_64 安装包及 SHA-256 校验文件，glibc 基线 2.31。

验证：Rust 1.99.1/macOS ARM64 的 fmt、严格 Clippy、19 项逻辑测试通过；Node 24/pnpm 12.6.0 的冻结锁文件安装、前端 lint、测试、类型检查与构建通过。两个 Linux 架构完成交叉编译，使用官方 v3.47.0 CLI 打包并校验清单、ELF 架构及文件摘要。

完整 CPR v3.47.0 宿主安装、旧版升级状态保留和 Linux 二进制运行尚未验证。请在隔离环境按 [宿主测试清单](https://github.com/Ming-321/CPR-key-reset-follow/blob/main/docs/local-release.md#用户宿主测试清单) 确认。
