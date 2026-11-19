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

# 桌面订阅代理（第一版）

桌面与手机共用同一套 Tailwind 响应式界面，宽屏使用侧边导航，窄屏使用底部导航。默认进入「订阅节点」，不要求腾讯云密钥，也不会自动查询、购买云主机。「云上主机」保留为第二种来源，页面导航不会停止或切换代理。点击「购买主机」时先停止当前代理及心跳，购买成功并等待远程服务就绪后自动启动云主机本地代理；已有主机可手动连接。真正连接订阅成功后才停止旧云主机心跳；只停止代理不会销毁云主机。

- 仅一个订阅链接；支持明文 URI 列表或 Base64 列表。更新成功覆盖节点；下载或解析失败保留旧节点和连接。暂不支持 Clash YAML。
- VMess AEAD（alterId=0），TCP / WebSocket，支持外层 TLS 且验证证书。支持 AES-128-GCM、ChaCha20-Poly1305；旧 alterId、其他传输或协议在列表标记暂不支持。
- 本地默认监听 `0.0.0.0:7893`，提供 `http://127.0.0.1:7893` 及局域网 IP 的 HTTP 代理地址：普通 HTTP 和 HTTPS CONNECT，暂不提供 SOCKS、TUN 或 UDP。启动前会检测该端口的现有监听，发现占用会提示先关闭其他代理，避免 macOS 的 IPv4/IPv6 监听共存接走其他程序的流量。
- TCP 测试每个节点三次，至少两次成功，取中位数；仅允许低于 500 ms 的节点连接。多个 DNS 地址错开并发尝试，避免首个地址失效拖住后续地址；DNS 查询另设超时。连接前还会通过 VMess 请求 example.com 确认实际转发。
- 全局模式仍直连局域网；绕过模式结合大陆域名和 IPv4/IPv6 网段，并对其他域名作有超时的本地 DNS 判断，失败时走代理。绕过模式需要本地 DNS，可受 DNS 污染影响；并非完整 GeoSite/GeoIP 引擎。大陆 IP 使用排序合并后的专用区间表，通过二分查询；不内置规则数据。首次无缓存时绕过大陆不生效（仍保留局域网直连），代理连接后通过该代理下载；之后每次连接检查上游提交版本，仅下载变化的文件。大陆域名规则也使用相同的外部下载流程。下载、解析和校验成功后原子提交缓存并热更新，失败保留旧版；全过程写入 Rust 日志。来源与许可证见 `src-proxy/data/SOURCES.json` 及同目录许可证。
- 订阅及节点凭据保存在应用数据目录 `subscription.json`，Unix 文件权限 0600；节点列表 IPC 不返回 UUID。订阅地址输入框明文显示，日志不输出地址 token 或 UUID。文件没有加密，仍需保护本机账户。
- macOS 自动系统代理通过 networksetup 保存并修改 HTTP/HTTPS 配置，断开、正常退出和下次启动时恢复备份；权限不足会显示错误。Windows 有系统代理适配，但本版只在 macOS 实机验证。系统代理修改及恢复没有在开发机自动执行，以免改动已有代理。

VMess 使用 Rust 库 leaf（Apache-2.0），锁定 commit `5e8d94759ce983eb8ac226c315024b55ff2bb7b0`，不下载或启动 V2Ray/Xray 二进制，只启用 VMess、WS、ring AEAD 和必要的配置功能。构建依赖 libclang；上游 leaf 使用 bindgen 和自带 protobuf 编译器，首次构建会较慢。升级时需重新验证真实节点互通。

## 开发和 Chrome CDP 联调

正常开发执行 `pnpm exec tauri dev`；单独运行 Rust 时先在根目录执行 `pnpm dev`，再在 `src-tauri/` 执行 `cargo run`。若环境里的 LIBCLANG_PATH 指向非 Apple 工具链，在 macOS 使用：

```sh
LIBCLANG_PATH="$(xcode-select -p)/Toolchains/XcodeDefault.xctoolchain/usr/lib" cargo run
```

macOS Tauri 使用 WKWebView，Chrome CDP 不能直接控制原生窗口。联调时额外设置 `CLOUDTUN_DEV_BRIDGE=1` 启动 Rust，再用 Chrome 打开 `http://localhost:1420`。调试桥只在 debug 构建中编译、显式启用后监听 `127.0.0.1:1422`，只接受本地开发页面 Origin 和白名单命令；页面通过它调用真实 Tauri 后端，没有模拟节点或模拟转发。发布构建没有此桥。不要把开发服务暴露到公网。

建议用独立 Chrome profile 加 `--remote-debugging-port=9222`，先关闭页面「设置系统代理」再操作；手动 curl 显式指定 `--proxy http://127.0.0.1:7893 --noproxy ''`，避免环境代理干扰。验证订阅更新、TCP 测速、选节点、HTTP/HTTPS 转发、模式切换、错误更新保留节点和停止连接。确认流量属于 CloudTun 进程，尤其在本机已有其他代理时。

验证命令：`pnpm build`、`pnpm exec oxlint --type-aware`、在 `src-proxy/` 执行 `cargo test --locked`，以及在 `src-tauri/` 执行 `cargo check --locked`。Rust 测试使用本地端口和虚构节点，不依赖私人订阅或公网服务。Jinge 不采用 React 的 render/ref 约束，oxlint 中关闭了对应的 immutability、refs 两项 React 检查。

## 统一设置与监听配置

菜单「设置」包含本地监听（端口、是否允许局域网访问）、GEO 规则和开机启动。腾讯云密钥、主机参数移入「云上主机」页面的「云主机设置」弹窗，首次未配置时页面提示先设置。各表单保存后生效。

监听配置由 Rust 保存到应用数据目录 `proxy-config.json`，默认 `listenHost=0.0.0.0`、`port=7893`。关闭局域网访问后使用 `127.0.0.1`。设置无需保存按钮：端口编辑防抖 400 ms 自动保存，局域网开关即时保存。运行中重新绑定监听，保留已建立的转发连接；绑定失败保留原设置并显示错误。订阅与云主机共用此配置。代理地址仅在侧栏底部展示，本机与局域网地址各提供复制按钮及成功提示，订阅页和云主机页不重复显示地址卡片。网络变化后可切换页面刷新地址。

自动系统代理始终指向本机地址及当前端口。系统代理恢复备份记录本次应用的端口，兼容旧版固定 7892 的备份，修改监听端口不影响原配置恢复。

`desktop.css` 已删除，统一页面使用 Tailwind 工具类及断点，手机安全区使用 CSS env。移动端目前整合页面与云主机 VPN 入口；订阅连接内核仍待后续接入，界面有明确提示，不会向手机端调用尚未注册的桌面订阅命令。

## 云上主机购买与代理切换

已移除自动购买开关及页面加载时的购买逻辑，保存设置时忽略旧版 `autoProxy` 字段。只有「购买主机」按钮调用购买接口；购买前停止当前代理，重复购买请求会被忽略。普通页面查看或刷新已有主机只更新状态；应用启动检查发现已有主机时，会自动激活云上主机菜单并等待服务就绪后连接本地代理。购买流程中的就绪检测会自动启动本地代理；若用户在等待期间主动改用订阅或断开连接，则取消这次自动连接，避免延迟完成的云任务覆盖新选择。

连接已有云主机前，Rust 会直接验证远程服务就绪；失败时保留当前代理。云主机探活和心跳请求绕过环境代理。销毁一个未被当前代理使用的云主机，也不会停止正在运行的订阅代理。

前端回归测试通过 `pnpm test` 运行，使用模拟依赖验证浏览不切换、购买前停止、服务就绪后自动连接、停止失败不购买以及新选择取消旧流程，不会真实购买云资源。

## 后台日志

Rust `Logs` 状态统一保存最近 1000 条日志（按写入顺序淘汰最旧条目），保留编号和时间戳。代理、心跳和通知的 `emit_log` 先写入缓冲再发事件；UI 操作日志通过 `tauri_append_log` 写入同一缓冲。每条文本最多约 8 KiB，避免异常大消息无界占用内存。

UI 初始化先订阅 `log-entry`，再读取 `tauri_get_logs` 快照，按编号合并去重并保持顺序。开发浏览器通过调试桥定时读取同一 Rust 快照；无独立的前端日志来源。后台会话编号用于避免重启后新旧日志混合。关闭或重建 UI 不影响后台日志，完全退出 Rust 后台后清空；当前不写磁盘。

日志页按本地时间显示毫秒时间戳、来源标签与正文；错误/警告突出显示。只在滚动条原本位于底部时跟随新日志，向上阅读时保留可见行的位置，标题按钮可返回底部并恢复跟随。条目淘汰时尽量保持当前可见日志及行偏移。

日志测试覆盖 1000 条上限、重复读取不丢失历史、并发写入顺序，以及超长 Unicode 消息截断。

大陆规则缓存位于应用数据目录 `china-rules/`：`current.json` 记录上游 IP/域名提交 SHA 与文件 SHA-256；版本目录保存下载的原始文件。启动时验证缓存后只加载大陆数据。IPv4/IPv6 区间合并存入紧凑数组，域名以后缀哈希集合匹配。上游版本通过 GitHub API 查询（失败时回退至 GitHub 提交 Atom 源，避免共享代理出口 API 限流），文件从固定提交下载，支持超时、取消和大小限制。

设置页的「GEO 规则」展示 Rust 保存的加载状态、IP/域名提交版本和最近一次更新结果；可经当前代理手动下载更新；手动操作始终重新下载全部规则（版本相同也下载），自动更新仍按版本判断。下载时按三个文件分别显示实际接收字节数和可用的 Content-Length，大小未知时显示不定进度。状态在 Rust 中维护，关闭设置页后更新继续进行，重新进入可继续查看。自动与手动更新共用互斥任务，避免重复下载。

云上主机与订阅代理共用顶部的全局/绕过大陆分流模式和同一个大陆 GEO 规则缓存。分流切换直接更新运行中的路由，后续请求立即使用新模式，已建立的连接保持运行。云主机设置只保留密钥和主机配置；旧版 proxyRules 不再加载，旧 subscriptionRoutingMode 迁移到全局 routingMode。命令行代理的自定义规则接口保留。

规则文件下载允许最多 5 分钟，版本元数据请求单独限制为 30 秒，连接超时保持 10 秒；慢链路下载期间保留旧规则并持续展示进度，停止代理会取消下载。

桌面端腾讯云管理 API 在 Rust 中签名、显式 no_proxy 直连（连接 5 秒、完整请求 20 秒），日志记录操作名、是否成功和耗时，不记录密钥和请求体。主机信息查询与服务就绪探测分离，前者完成即可展示页面；探测复用 Rust 直连逻辑，10 秒超时。查询失败清理 loading、保留已知主机并提供重新加载，避免把网络失败误当成无主机。

启动时先查询已有云主机；存在则优先激活并连接，不自动购买。没有主机或查询失败则进入订阅页。订阅页第一次激活时自动更新已保存的链接并测速，仅在没有云主机优先任务、没有其他代理连接且上次选择的节点仍可用时自动连接；无链接、更新失败、节点消失/不支持/延迟不合格均不连接。此检查和首次订阅更新在同一前端会话只执行一次。

前端启动/云主机生命周期测试使用 `pnpm test`（`bun test --isolate`），每个测试文件隔离模块 mock，避免影响其他文件。覆盖启动优先级、单次订阅更新、记住节点自动恢复、失效节点跳过和用户主动操作取消自动连接。

云主机检测在 `main.tsx` 完成日志、配置和本地状态初始化之后、挂载 App 之前执行；检测完成后将初始菜单传入 App/Layout，首帧直接显示目标页面。远端服务探测与本地代理连接在后台继续，不阻塞整体界面的展示。

本地 HTTP 入口支持明文 WebSocket 升级（http/ws 绝对 URI）及 HTTP CONNECT 内的 WSS。仅对合法的 WebSocket Upgrade 保留必要握手头，仍移除代理认证与其他逐跳头；101 之后双向转发原始字节，任务受代理关闭信号统一管理。SSE 响应体直接流式转发，不等待完整响应。回归测试覆盖升级握手、双向帧、停止关闭升级连接，以及首个 SSE 事件在响应完成前到达。

实机流式验证（2026-11-19）：经本地 7893 → 硅谷云主机 → 临时受控端点，HTTP/HTTPS SSE 分段到达；HTTP/ws 明文升级和 WSS 均通过文本、128 KiB 二进制、Ping/Pong、正常关闭测试。SSE 事件间隔及 WS/WSS 无应用数据间隔 70 秒后仍能继续收发。测试端点及自签名测试证书已清理；没有修改生产 TLS 校验行为。


## WSS v3 / TOFU（服务端 0.2.0）

云端链路已替换为 WSS，删除 XOR 和实例 ID 派生密码。实例创建前生成独立随机 32 字节访问凭据，通过初始化脚本的 `CLOUDTUN_ACCESS_TOKEN` 注入服务端；客户端按实例 ID 私有保存。更换设备或缓存丢失时，可经腾讯云 `DescribeInstancesAttributes(UserData)` 恢复凭据，需要对应 API 权限；不记录请求体、凭据或私钥。

服务端首次启动在 `--data-dir` 内生成并持久保存 `identity.json`（自签名证书、私钥和访问凭据，Unix 权限 0600）。常规重启复用身份；现有身份损坏或指定凭据变化时直接失败，不静默生成新身份。证书无需域名或公共 CA。

客户端按实例 ID 在私有目录 `cloud-trust/` 保存证书 SHA-256，首次连接接受服务端证书，完成 TLS 握手和签名校验后才写入信任记录；后续精确校验证书，不一致时拒绝连接。并发首次握手也必须收敛到同一证书。此 TOFU 策略不提供首次连接的抗中间人保障，也不使用公共 PKI 的域名/有效期校验；已有证书固定后仍严格验证 TLS 握手签名。信任文件损坏时失败，不当作首次连接。新实例 ID 独立建立信任。TCP、UDP、探测和心跳均使用该共享 TLS 层，没有明文降级。

服务端 `--standalone` 可在本地运行，无需腾讯云账号且不会回收云资源。完整测试命令：

```sh
python3 scripts/test-wss-local.py
```

脚本自动编译并启动 Rust 服务端、本地 HTTP 代理和受控 HTTP/HTTPS/SSE/WS/WSS 目标，使用动态本地端口，不占用 7893、不购买主机，退出时清理临时进程、私钥和信任文件。默认运行 70 秒空闲验证；`--quick` 改为 1 秒，`--skip-build` 复用已有可执行文件。

附加回归测试：在 `src-server/` 执行 `cargo +nightly test --locked`，覆盖证书首次信任与复用、换证书拒绝、并发固定证书、错误凭据、完整 HTTP→WSS→SSE/WS、TCP 半关闭、大数据背压及 UDP 边界。

**发布要求：** WSS v3 与已发布的旧明文 WS 服务端不兼容。先通过既有手动 CI 发布 `src-server/Cargo.toml` 的 `0.2.0`，再创建新实例。旧实例缺少新初始化凭据时会显示升级/重建提示，不会回退到旧协议。Android 的 Kotlin/JNI 参数已同步为实例 ID、随机访问凭据与私有信任目录；共享 VPN 层已通过 aarch64-apple-ios 编译检查，但本轮没有 Android/iOS 真机测试。
