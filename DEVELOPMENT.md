# 服务端构建与发布

服务端直接分发静态编译的 Linux x64 Rust 可执行文件，不再使用 npm 包或 Node.js 启动入口。云主机需要 Linux x86_64、curl、sha256sum 和系统 CA 证书；当前初始化脚本面向 Ubuntu 云镜像。

## 手动发布 GitHub Release

工作流位于 `.github/workflows/cloudtun_server_ci.yml`，显示名称 `build_and_publish_server`，只监听 `workflow_dispatch`。在 GitHub Actions 页面选择该工作流，选择代码分支后点击 Run workflow 即可，不需要输入版本号。

版本读取 `src-server/Cargo.toml` 的 `[package].version`。发布前更新版本，并在 `src-server/` 执行 Cargo 命令同步和提交 `Cargo.lock`。当前仅发布稳定的 `major.minor.patch` 版本。

CI 会在 `src-server/` 运行 `cargo test --locked`，然后构建 `x86_64-unknown-linux-musl` release 程序、strip 符号、确认没有动态加载器，并测试 `--version`、`--help`。随后创建 `server-v<版本号>` Release，上传：

- `cloudtun-server-linux-x64`
- `cloudtun-server-linux-x64.sha256`

Release 先作为草稿创建，附件上传完成后再公开并标记为仓库的 Latest。已公开的版本不覆盖；再次发布需要更新 Cargo 版本。相同 commit 的上传失败草稿可以通过重跑工作流继续。发布使用仓库自带的 `GITHUB_TOKEN` 和 `contents: write` 权限，无需额外 Secret 或 npm Trusted Publisher。工作流使用所选 commit 创建 tag，不会提交版本改动。

## 云主机初始化

`src/assets/shell-template/agent.sh` 从以下固定入口下载最新程序及校验文件：

```text
https://github.com/YuhangGe/CloudTun/releases/latest/download/cloudtun-server-linux-x64
https://github.com/YuhangGe/CloudTun/releases/latest/download/cloudtun-server-linux-x64.sha256
```

校验通过后才替换 `/home/ubuntu/cloudtun-server` 并使用原有腾讯云参数后台启动，日志仍位于 `/home/ubuntu/cloudtun.log`。无需安装 Node.js、npm 或 Rust。必须先成功发布一版带有以上两个附件的 Release，再创建使用此脚本的云主机。

该入口指向整个仓库的 Latest Release；以后若发布桌面客户端等其他 Release，需要确保它们不抢占服务端的 Latest 标记，或相应改用独立的服务端下载入口。GitHub Release 下载仍取决于云主机到 GitHub 的网络可达性。

## 手动下载与启动

```bash
curl -fL --retry 5 -o cloudtun-server-linux-x64 https://github.com/YuhangGe/CloudTun/releases/latest/download/cloudtun-server-linux-x64
curl -fL --retry 5 -o cloudtun-server-linux-x64.sha256 https://github.com/YuhangGe/CloudTun/releases/latest/download/cloudtun-server-linux-x64.sha256
sha256sum --check cloudtun-server-linux-x64.sha256
chmod +x cloudtun-server-linux-x64
./cloudtun-server-linux-x64 --help
./cloudtun-server-linux-x64 --secret-id '<SecretId>' --secret-key '<SecretKey>' --region 'na-siliconvalley' --cvm-name '<主机名称>'
```

服务端仍保留十分钟无心跳及实际代理流量后尝试回收临时腾讯云主机的逻辑。
