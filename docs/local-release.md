# 本地构建与发布

从待发布提交构建。Linux 原生构建与交叉编译命令如下。需要 Rust 1.99.1（含 rustfmt、clippy）、Node 24、pnpm 12.6.0，以及目标架构的 C 编译器。ARM64 主机上的 x86_64 交叉编译使用 x86_64-linux-gnu-gcc。

## 检查与编译

在插件仓库执行：

```sh
cargo +1.99.1 fmt --check
cargo +1.99.1 clippy --all-targets --locked -- -D warnings
cargo +1.99.1 test --locked
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend lint
pnpm --dir frontend test
pnpm --dir frontend build
cargo +1.99.1 build --release --locked --target aarch64-unknown-linux-gnu
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
  cargo +1.99.1 build --release --locked --target x86_64-unknown-linux-gnu
```

如设置 CARGO_TARGET_DIR，后续二进制路径需相应替换。交叉编译通过不代表目标架构运行测试通过。

## macOS 交叉编译

macOS ARM64 可使用 Rust 1.99.1、cargo-zigbuild 和 Zig 构建 Linux 安装包。先安装两个 Rust target，再分别执行：

```sh
rustup target add aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu
cargo zigbuild --release --locked --target aarch64-unknown-linux-gnu.2.31
cargo zigbuild --release --locked --target x86_64-unknown-linux-gnu.2.31
```

这将 glibc 兼容基线设为 2.31；输出仍位于不带 `.2.31` 的 target 目录。使用 `file` 核对 ELF 架构，不能把 macOS 测试通过视为 Linux 运行测试通过。打包 CLI 可在 macOS 从同一固定宿主提交构建。

## 打包

检出官方 CPR v3.36.0 提交 `5ae38dc0c55c192815ef605dde6c05fd767a2c28`，在其目录运行：

```sh
cargo +1.99.1 build --manifest-path backend/Cargo.toml -p codex-proxy-plugin-cli --locked
```

返回插件仓库，设置 CPR_PLUGIN 为上述构建产生的 cpr-plugin 绝对路径：

```sh
for target in aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu; do
  "$CPR_PLUGIN" package --manifest plugin.json \
    --binary "target/$target/release/key-reset-follow" --target "$target" \
    --resource-map web=frontend/dist --output-dir dist
done
(cd dist && sha256sum --check *.sha256)
```

检查包内 manifest 的 RPC 为 7、management 为 2，并核对实际二进制架构和动态库要求。账号保护 middleware 为 6 且保持 attempt，Key 重置 maintenance 为 1。

## 发布

CI 仅手动触发，不在 push、PR 或标签事件上自动构建。本地全部检查成功后，将真实结果及未验证范围记入 PR；有 check 分支保护时，在用户授权下回报相应提交状态，再按仓库规则合并。不得给未检查的提交报告成功。

标签对应实际产物源码，且与 plugin.json 版本相同。创建正式 GitHub Release，上传两个 tar.gz 及对应 sha256；不覆盖已有标签或附件。上传后下载公开附件并核对校验值。

## 用户宿主测试清单

在隔离 CPR v3.36.0 环境用合成数据检查：

- 从旧版更新后能启用，管理页面可读取与保存，原有设置、关联、状态和中间件失败策略保留。
- Key 重置：换周只重置一次，未知结果不自动重试。
- 账号保护：低于阈值拦截、等于或高于阈值放行，新周期恢复并按规则重新抽取随机阈值。
- 使用额度重置卡提前重置后，两个插件仍按新周窗口正确判断。

不要卸载重装以规避升级问题。发布说明分别列出本地验证和用户负责的宿主验证。

