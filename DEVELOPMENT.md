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
- TCP 测试每个节点三次，至少两次成功，取中位数；仅允许低于 800 ms 的节点连接。多个 DNS 地址错开并发尝试，避免首个地址失效拖住后续地址；DNS 查询另设超时。连接前还会通过 VMess 请求 https://www.google.com/generate_204，校验证书并确认返回 HTTP 204。
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

`desktop.css` 已删除，统一页面使用 Tailwind 工具类及断点，手机安全区使用 CSS env。Android 已接入订阅与云主机 VPN；iOS 订阅内核尚未接入，界面保留提示与禁用入口。

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


# Android 构建与 VPN

Android 保留两份原生库：Tauri 的 `libcloudtun_lib.so`，以及 VPN Service 单独加载的 `libcloudtunvpn.so`。调用链仍为前端 Tauri command → Rust mobile plugin → Kotlin `CloudTunPlugin` / `CloudTunVpnService` → JNI。VPN 内核自建 Tokio runtime，不依赖 Activity 或 WebView 保持活跃；前台服务通知负责维持 Android 服务生命周期。

需要 JDK 21、Android SDK 36、NDK 与对应 Rust Android target。macOS 的 `scripts/android.ts` 自动查找 SDK、NDK 和 JDK 21，并设置 Leaf bindgen 所需的 NDK sysroot。可以用环境变量 `JAVA_HOME`、`ANDROID_HOME`、`NDK_HOME` / `ANDROID_NDK_HOME` 指定已有安装。

```bash
pnpm build-apk                         # arm64 release APK
pnpm build-apk --debug                 # arm64 debug APK
pnpm dev-android                      # Tauri Android 开发模式
pnpm exec bun scripts/android.ts build --debug --apk --target x86_64
```

Gradle 的对应 `rustBuild<架构><配置>` 任务会先运行 `vpnBuild<架构><配置>`，调用 `scripts/build-android-vpn.sh` 编译并复制独立 VPN 库，再进行 JNI 合并与 APK 打包。两份库各自保留 Cargo 编译缓存，按本次构建的架构选择目标；不再要求手动先构建 `src-vpn`，也不再使用旧 `rust-android-gradle` 插件。Gradle wrapper 使用 LF 换行，避免 macOS/Linux 的 `sh\r` 启动错误；仓库内不固定个人本地代理端口。

Android 支持云主机 WSS v3 与 VMess AEAD（TCP/WebSocket、可选 TLS）的 TUN TCP/UDP 转发，保留虚拟 DNS 及全局/绕过大陆分流模式，VPN 接管全部应用（排除 CloudTun 自身）。TUN 与本地 HTTP 入口复用 `src-proxy` 协议和分流实现。Tauri 将 VPN 配置原子写入私有目录 `vpn-config.json`（Unix 0600），仅通过插件和 Intent 传文件路径，避免把完整 GEO 数据塞进 Binder 消息；独立库通过 JNI 读取配置，更新规则与模式。应用自己的 UID 排除在 VPN 外，避免上游连接回环。HTTPS 校验证书与主机名，Android 的 Rust HTTP 请求使用 Mozilla 根证书，不依赖额外的 JVM 证书组件。

首次连接由 Tauri `ActivityCallback` 接收系统 VPN 授权结果，保留原调用参数；授权被拒、TUN 创建或 JNI 启动失败时不报告连接成功。停止、系统撤销和内核退出会清理 TUN、独立 runtime 与前台通知。Android UI 定期读取真实服务状态，通知栏断开后会同步界面。

Vite 的 `envDir` 是 `scripts/`。本地默认订阅写入 `scripts/.env.local` 的 `VITE_SUBSCRIPTION_URL`；该文件由 `*.local` 忽略。链接会嵌入生成的前端资源，适用于本地验证包；不要将带个人订阅的 APK 对外分发。未配置本地覆盖时仍可在订阅页面输入链接。


本次 Android 模拟器验证范围：订阅自动拉取/测速、首次 VPN 授权、Chrome 在 CloudTun 退到后台后访问 `https://www.google.com.hk/`、切换订阅节点、切换分流和停止清理。未实连云主机。Google 香港最终复测返回 HTTP 200，Chrome 导航协议为 HTTP/3，已验证经 VPN 的 UDP/443；TCP/HTTPS 也通过实际页面访问验证。STUN/19302 探测未收到成功响应（同节点在宿主机直接测试也失败），不将该项算作通过。

覆盖安装后的首次启动曾出现一次原生 `FORTIFY: pthread_mutex_lock called on a destroyed mutex` 退出；系统没有提供可定位的崩溃堆栈。重新启动以及强停后的冷启动复测均通过，原因尚未定位，不能据此保证真机稳定性。


## Android 云主机实测（2026-11-20）

从本机 Downloads 的 `secretkey.csv` 读取 SecretId/SecretKey，写入已忽略且权限为 0600 的 `scripts/.env.local`，使用现有 `VITE_SECRET_ID` / `VITE_SECRET_KEY` 默认配置入口。不在文档中记录密钥；本地验证 APK 会包含环境默认值。

真实 Android UI 流程完成：使用既有 VPC/子网/安全组创建硅谷 `SA5.MEDIUM2` 临时竞价实例，cloud-init 下载 `server-v0.2.0` 并启动 WSS v3，安卓自动建立 VPN；CloudTun 退到后台后，Chrome 访问 Google 香港返回 HTTP 200 / HTTP/3，IP 查询结果与测试机公网 IP 一致。断开后运行状态关闭、流量快照为空、TUN 文件描述符释放、系统 VPN 不再活动。随后通过 Android UI 销毁测试机，并用腾讯云 API 确认实例列表为空；既有网络资源保留。

实测修复：云主机校验只检查云资源相关必需字段，订阅候选列表的合法 null 值不再阻止购买；Android 与桌面共用主机就绪后的自动连接流程。增加空候选配置、非法带宽及 Android 购买/已有主机重连回归验证，前端共 27 项测试通过。

后续已按用户确认将 Android 自动切换和连接通知迁入 VPN Service 管理的独立运行时，见下节。桌面端保留原有前端自动切换流程。


## Android Service 自动切换、通知与流量

启用自动切换后，`CloudTunVpnService` 持有的 `libcloudtunvpn.so` / Tokio runtime 负责检测，不依赖 WebView 定时器。每 15 秒对当前订阅节点作最长 5 秒的实际 VMess HTTPS 探测；连续三次失败后，按候选节点的 TCP 延迟排序（仍要求低于 800 ms），再逐一验证 Google 204 的 TLS/HTTP 响应，只有验证成功才切换。订阅候选完全不可用时保留当前连接配置、每轮继续检测，异常通知只在进入故障状态时发一次，恢复时另发一次。云主机保持原生心跳，不自动跨来源切到订阅。

代理内核的动态出口在切换时取消旧出口会话，新连接使用新节点；不重建 TUN，虚拟 DNS 映射、会话编号和累计流量保留。候选策略更新带版本校验，关闭自动切换或替换策略后丢弃旧探测结果；相同策略的重复同步不会重置失败计数。手动停止/撤销 VPN 会取消整个原生 runtime 并释放 TUN，不因候选网络恢复而重新连接。

`android-failover.json` 记录启用状态及候选 ID，`vpn-failover.json` 通过私有文件向独立库同步完整候选配置；文件均为 0600。Service 经 JNI 读取公开状态和流量快照，界面恢复后从 Service 同步实际节点，并更新本地 HTTP 出口。已运行的 Android 会话恢复 WebView 时不自动刷新订阅并重启 VPN。

运行通知显示节点名称或云主机 IP、上传下载合计速率和本会话累计有效载荷字节（不含外层加密/协议开销）。每两秒采样，按真实 elapsedMs 差值计算速率，内容未变化时不刷新通知；会话更换、采样回退时速率重置为零。自动切换不清空累计。运行通知使用静默更新，切换成功、候选不可用及恢复使用独立事件通道；手动断开移除运行通知。通知权限被关闭时不影响转发。

仅在订阅自动切换开启时持有 `PARTIAL_WAKE_LOCK`，支持普通熄屏时继续检测，关闭自动切换、停止服务时释放。流量通知本身不额外申请唤醒锁。持锁可能增加待机耗电；前台服务/唤醒锁不等于绕过所有系统 Doze 或厂商省电策略，长时间深度省电仍需真机验证。

回归验证入口：

```bash
pnpm test
cargo test --manifest-path src-proxy/Cargo.toml --lib
cargo test --manifest-path src-vpn/Cargo.toml --lib
# 安装 debug APK、确保缓存订阅含至少两个可用 VMess 节点后运行：
node scripts/test-android-failover.mjs
```

集成脚本使用本机可控 TCP 中继及 `adb reverse` 接入真实节点，不需要 root，也不添加生产故障开关。通过实际关闭中继连接模拟当前节点断联；冻结 WebView、将应用放到后台并关闭屏幕后，验证 Service 切换及系统通知，再验证 Google 香港 HTTP 200、流量累计保留、全部候选异常、网络恢复、手动停止与 TUN 释放。额外重载 WebView 并校验会话编号不变。结束时恢复原订阅、设置和自动切换策略，移除本次端口转发，不创建云资源。

通知速率计算的 JVM 测试位于 `app/src/test/.../VpnTrafficDisplayTest.kt`，覆盖非固定采样间隔、空闲、会话重置、重复及回退采样。

Android 真机修复：删除历史 `mobileProxyMode/mobileProxyApps` 设置、启动参数、应用枚举插件和 QUERY_ALL_PACKAGES 权限；旧设置加载时丢弃这两个字段。VPN 始终接管所有应用并排除自身，避免历史 Chrome 白名单导致 Edge/YouTube 完全绕过 VPN。vivo release 实测修正接管范围后 Edge 能打开 YouTube 首页，通知累计流量正常增长。

## PC 调试日志与下载

设置页新增「调试日志」开关，仅桌面端显示。默认关闭且不跨应用重启恢复；开启后显示「下载日志文件」，将一致的 JSONL 快照保存到系统下载目录，页面显示完整保存路径。关闭后停止写入，已经生成的文件保留；下载只读取日志文件，不包含应用配置/订阅缓存。

后台独立线程通过最多 2048 项的有界队列写入 app_log_dir/debug/debug-{0,1,2}.jsonl，单文件最多约 2 MiB，保留三个文件。代理转发任务不执行文件 I/O，队列繁忙时丢弃日志并累计数量，状态及导出头部可查看丢弃数与磁盘错误。导出与记录经过同一队列，导出前的记录先完成写入；使用排他创建，失败不会覆盖或删除已有文件。Unix 目录权限 0700、日志与导出文件权限 0600。

诊断条目包含毫秒时间戳、应用会话 ID；本地 HTTP 连接 conn 编号关联该连接的 req 请求编号、客户端地址、目标域名/端口、路由结果、出口地址和 VMess 传输/TLS 类型。记录建立/升级/传输/关闭阶段、耗时及上下行有效载荷字节，失败保留底层错误链；不解析 HTTPS 明文，不记录请求正文、请求头、节点 UUID/密钥或订阅配置，链接的账号、路径、查询参数以及显式密钥字段脱敏。额外的 proxy::debug 阶段日志只进调试文件，原有运行日志仍保留最近 1000 条。

验证：前端类型、lint、构建及 28 项测试；代理 23 项测试（包含真实本地连接拒绝的诊断上下文及认证头不泄漏验证）；桌面核心 14 项测试（包含关闭不写盘、导出一致性、脱敏、文件轮转、已有导出保护及磁盘错误）。Chrome CDP 通过 debug-only 本地桥调用真实 Tauri 命令，完成设置开关、下载文件及 JSONL/权限/脱敏检查。

PC 连接日志分级：CONNECT/WebSocket 隧道传输中的 UnexpectedEof 与客户端 HTTP 的 IncompleteMessage 记为 proxy::warn，运行日志以橙色展示，保留错误链、连接编号、耗时与流量。连接建立超时、认证/协议异常及其他传输错误维持红色；正常关闭和明确停止取消维持调试日志。前端优先使用显式级别，避免警告正文中的 `error`/`hyper::Error` 再次被误染成红色。直连上下文标记 outbound=direct；HTTP 处理完成改称 client.http.finished，避免将升级交接误认为隧道关闭。新增级别回归和真实不完整 HTTP 请求验证，前端 36 项、代理 25 项测试通过。

## 真连接延迟测速（替换 TCP 测速）

2026-11-23 起节点列表测速改为 v2rayN Realping 语义：通过每个节点独立的回环 HTTP 代理访问 `https://www.google.com/generate_204`，完成节点协议、目标 HTTPS 握手和完整响应读取。两次 GET 共用 5 秒预算，每次结束后间隔 100 ms，取最小正耗时；任一次请求/读取失败则整次测试失败。与所核对的 v2rayN 实现一致，不强制要求 HTTP 204。回环代理只绑定 127.0.0.1 的临时端口，不改变当前连接、系统代理或分流规则；测试完成、超时或任务取消时取消其运行时任务。关闭环境代理自动继承，避免测速意外走其他代理。

移除原 TCP 三次/800 ms 筛选：列表、手动选节点、连接前校验、启动恢复、PC 与 Android Service 自动切换统一使用真实转发探测和 5 秒边界。连接前不再重复做旧 TCP 筛选加单次 204 探测。失败显示「测试失败 / 超时」，界面说明真连接测试与 5 秒超时；连接时仍重新验证，避免沿用过时结果。

验证：代理 30 项、前端 37 项、VPN 10 项测试通过；桌面核心 cargo check、前端类型/lint/build 通过。新增真实本地 HTTP 响应验证两次完整读取、非 204 状态、第二次失败丢弃第一次成功、共享总超时，以及 900 ms 延迟通过；另验证 TCP 可达但不能 VMess 转发时不再返回有效延迟，以及 1200 ms 节点可启动恢复。
