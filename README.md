# CloudTun

> 基于云（腾讯云）的超低成本轻量网络代理方案

## 方案架构

1. 云主机的竞价实例极其便宜。
2. 在需要代理时使用腾讯云API自动购买主机，创建主机时传递UserData启动脚本下载并启动 CloudTun 服务端。
3. 实现一个超轻量的代理服务，该服务除了代理外，还会在连续10分钟没有客户端请求时调用腾讯云API销毁当前主机。
4. 实现一个超轻量的代理客户端和服务端通信，用于 macos/windows。
5. 实现一个超轻量的 vpn 流量转换器和服务端通信，用于安卓/IOS。
6. 使用 Tauri 框架实现跨端 GUI 软件。

## 代理协议

因为每次购买的主机 IP 都会变化，因此不需要 v2ray/shadowsocks 一类的混淆对抗型代理服务。且仅仅是为了自己使用，也不需要任何健壮性和稳定性，手撸一个基于 `websocket` 的转发方案即可。

客户端通过 websocket 发起链接时，通过普通的 http headers 传递代理目标，比如 `x-connect-host:baidu.com;x-connect-port:80;`，然后每收到 1kb 的 tun 流量包装成一条 websocket 二进制消息发给服务端。服务端每收到一条 websocket 二进制消息，就转发到这个目标的 tcp 连接上既可；同理，服务端收到目标 tcp 连接返回的数据，每 1kb 包装一条 websocket 二进制消息吐给客户端，客户端收到后把流量取出来再吐给 tun。

此外，在客户端向服务端发送 websocket 消息时，会使用最最简单的 `xor` 加密算法。密码为当前主机的 InstanceId 信息，补全为 16 位。加解密都使用 SIMD 并行指令加速。

## 研发构建

### 环境准备

1. 安装 rust 环境，安装 nightly channel 并默认切到 nightly。（SIMD 并行指令加速需要 nightly 环境编译）。
2. 安装 node + pnpm.
3. 项目根目录执行 `pnpm install`。 

### PC 平台

直接在根目录执行：

```bash
pnpm tauri build
```

### 安卓平台

用 `Android Studio` 打开 `src-tauri/gen/android` 目录，待 gradle sync 成功后，打开内置的 Terminal ，执行：

```bash
.\gradlew cargoBuild
```

这一步是构建 vpn 隧道转发服务,代码目录是 `src-vpn`。VpnService 是独立进程的服务，在它里面依赖的 rust 逻辑需要是独立的 lib 包，不能放在主项目的 rust 代码中。

然后构建主应用：

```bash
pnpm tauri android build
# 如果是开发，pnpm tauri android dev
```

### IOS 平台

TODO: 支持 IOS 平台。

所有网络代理和VPN流量转发的相关的逻辑都是 rust 实现，理论上 ios 平台的接入也没有屏障。