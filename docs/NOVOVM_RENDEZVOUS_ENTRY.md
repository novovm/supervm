# 自有寻址服务可执行入口

## 2026-10-14 平台持久端点上的原节点宿主

`product_rendezvous_owned::run_self_owned_node_on_endpoint_v1` 接受平台已有的
`IrohEndpointOwnerV1`，与原 `run_self_owned_node_v1` 进入同一个 `Node::serve`。
节点发现、NVMBOX01–05 分派、Finished、接收方能力、SQLite 落盘及维护均复用
原实现；此入口不 bind、不创建 Tokio runtime，不分离启动第二个调度器。

`IrohEndpointOwnerV1::with_owned_listener_v1` 要求平台托管的稳定传输密钥，
为整个服务期独占原端点的入站队列；另一个入站消费者立即拒绝，而不是抢收
彼此的连接。服务内部仍只允许一个待处理 accept。已独立授权的出站操作可并行，
共享原四连接上限；平台节点宿主最多使用其中两条入站连接，为设备自身留出
出站额度。专用 L3 入口仍保持原四连接边界，不降低已有容量。

停止服务只回收该服务的连接，端点最终关闭仍归平台 owner。取消后释放入站
所有权；端点 owner 被撤销后不可复活。30秒初始流预算没有变成网络通行许可，
旧有限操作不能取得连续权限，新服务仍经原协议显式认证。

该能力不自动将临时聊天端点升级为公开稳定节点，也不证明 UCA 已链上绑定。
Android 接线仍须在现有 Application owner 内托管用途密钥/本地受保护目录，
明确执行条件、贡献设置及身份曝光边界。贡献关闭/资源不足使用原
`MailboxServiceV1` 关闭新增义务，保留已有密文的 Fetch/Consume/TTL；不能用
停止监听或删除数据库冒充义务已经完成。实机 L4 和独立多副本另行验收。

## 2026-10-14 加密授权日志的有界压缩

同一 `ServiceLeaseFileV1` 保留 API 和密钥托管来源，新增 NVLSJ002 本地格式；
可读取旧 NVLSJ001，在达到 512 KiB 后的下一次 CAS 写入/确认中进行压缩。
16 MiB 仍是读取的硬上限，不靠扩大上限维持服务。压缩复制完整最新检查点，
不筛除过期历史，不改变签发序号、撤销墓碑、信任策略或旧 Publisher 迁移记录。
新格式头含压缩代次及前一日志尾摘要，并纳入首帧 AEAD 认证；后续仍是随机
XChaCha20 nonce 的链式加密帧。保留历史的安全状态与链摘要，不承诺保留每个
旧版本检查点的完整审计正文；旧备份回滚仍需要外部新鲜度依据。

写入顺序：验证当前全部日志及 expected → 私有同目录 `.compact` 文件写完整
检查点 → sync → 回读验签及逐字节相等 → 保持独立 `.lock` 所有权、关闭数据
句柄 → 同目录替换 → 刷元数据 → 重新打开/独占/验签 → 才继续追加新的授权。
Windows 复用固定 `atomicwrites=0.4.4` 的 write-through 替换；Unix 使用 rename
及父目录 sync。任何不确定写入停止当前 owner，不能返回新的签发结果。
`.compact` 从不作为恢复源；主文件丢失或损坏时拒绝启动，不能挑旧副本恢复。
只有主文件已完整验签后，才清理未发布的暂存文件。`.lock` 不随进程退出删除。

升级必须先停止旧二进制；原平台 owner 锁与新版日志锁共同保护唯一写者。
禁止混合新旧文件写者并发；旧二进制不识别 NVLSJ002，不能降级回写。平台目录
继续要求私有 ACL/无 reparse point。原身份、撤销和已签发历史不重置。

本机组件验证：54 项授权/持久化回归通过，覆盖六个真实子进程中断点（非掉电
模拟）、跨进程独占、重复压缩、有界大小、损坏/缺失主文件、失败后冻结和真实
签名状态的撤销恢复；scoped Clippy 通过。Windows 初次编译被 unsafe 禁令拒绝，
改用开源文件适配；随后实测发现打开的数据句柄使替换失败，关闭句柄且保持
稳定所有权后通过。两项测试误用第二句柄读取被锁文件也已修正，未降低锁。
这些结果不等于所有文件系统上的突然断电验收，实际节点升级/持续运行另记台账。
实现参考：[atomicwrites 0.4.4](https://docs.rs/crate/atomicwrites/0.4.4/source/src/lib.rs)、
[Windows MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)。

**后续增量：** [独立签名发布进程](NOVOVM_BOOTSTRAP_PUBLISHER.md)已接线，
可在本机自动授权新任期；Windows 系统密钥托管、常驻 owner 与提前准备下一任期已接线；
[运行与验收边界](NOVOVM_RENDEZVOUS_WINDOWS_OWNER.md)。下文保留原有界入口的
交付记录，不能将新平台入口的实现等同于已完成系统安装/重启和手机跨网。

2026-10-11，输入 `68eacb80`，认领 `09f5d39b`。本片交付独立进程入口及
通用协议归并。工程普通连接，没有匿名资格；没有改 NOV 交易、AOEM 或共识。

## 唯一实现与真实调用链

```text
supervm-relay --runtime rendezvous-v2 <owner-config.json>
  → network::product_relay_launch（显式选择，不回退 WSS / V1）
  → product_rendezvous_daemon::run_rendezvous_cli_v2
  → 原 IrohScope（本地授权、显式 relay / DNS、期限和取消）
  → 新传输公钥 / 新实例 → 节点签名 proposal → 独立发布者签名来源
  → ServiceLeaseBook + ServiceLeaseFile（固定信任、加密日志、文件锁、CAS）
  → ServiceLeaseHost / NVLSRQ02（先落盘再提供短租约）
  → bound_session（TLS exporter、身份签名、双向 Finished）
  → discovery_control（原寻址 RPC）→ Opaque RendezvousStore
```

`bound_session.rs` 与 `discovery_control.rs` 从 NOVOCHAT 迁回 network，原
线格式、域分隔字符串、消息类型和安全回归保留。历史 `novochat.endpoint-rpc`
名称不是新的聊天节点协议；更名需要显式版本迁移，本次不更改。NOVOCHAT 仅
保留 `pub use` 兼容入口，未来其他客户端可直接复用本实现。服务记录是有限期
不透明寻址密文，不是聊天历史或钱包账户。

## 运行契约

构建：设置本仓 `CARGO_TARGET_DIR`、`CARGO_BUILD_JOBS=1`，执行
`cargo build -p novovm-relay --features iroh-transport`。未启用该 feature 的
历史程序拒绝此运行方式；仍保留显式旧 WSS 入口。node 中的既有 relay bin
共用 selector，但本片只验收轻量 `supervm-relay`，未编译/验收金融节点整体。

配置必须显式 `version:2`、`engineering_direct_allowed:true`；包括固定发布者
公钥（1–8、阈值）、允许客户端公钥（1–16）、批准的 relay/DNS、可选绑定地址、
绝对 journal/source 路径、`enroll`、record ID、grant sequence、`run_for_ms`
及 `max_sessions`。没有自动信任网络返回的根，没有公共 relay 默认值。

平台 owner 通过重定向 stdin 传入 `NVKEYS02` + 32 字节独立节点签名种子 +
32 字节独立存储材料；不放命令行、环境变量或 JSON。重定向 stdout 第一行
是 `awaiting_authorization` 和节点自签 proposal，只交给本地受信 owner，
不代表已被网络接受。单独的发布者将 proposal 放入已有 SignedBootstrapManifest，
原子发布到 source 文件。不存在来源时有界等待，错误/旧来源直接失败。
配置、管道、来源文件和目录 ACL 仍由安装 owner 保护；仅检查非终端输入输出
不能证明私有 IPC 或平台密钥托管。当前未提供 Windows DPAPI/系统服务安装 owner，
不得把原始密钥文件重定向冒充已完成安全安装。

首次 `enroll:true` 用 create-new 建日志，已有文件就拒绝；`enroll:false` 必须
打开既有日志，丢失/错误密钥/锁冲突不会重建。每次进程重启使用新传输密钥和
随机实例，需要新发布者授权，grant sequence 必须高于既有签发历史。旧记录
不能复活旧寻址实例或重置序号。整个旧备份回放仍需外部 freshness anchor。

## 资源、撤销与剩余工作

本运行入口是一段 **5–110 秒有界服务任期**，最多 64 次接入、4 个并发流、
V2 前导 3 秒、每流最多 30 秒。沿用原握手/RPC/存储上限及 30 秒记录 TTL；
250 ms 本地 owner 维护到期清理与来源文件更新，不重复写入相同来源。已接受
撤销先持久保存；载体每次授权检查同时核对当前服务授权，终止全部旧流。
日志上限、丢回包序号恢复、错误身份、旧线格式拒绝保持原规则。报告只有
连接总数/完成/拒绝计数，不输出客户端身份、寻址能力或载荷。

它还不是无限常驻服务。跨任期的自动发行、可信源推送、持久平台密钥 owner、
崩溃重启监督、长期日志轮转及部署限流仍待交付；不能循环自签来绕过发布者
授权。Windows 现有持久 TCP 443 WSS 防火墙规则保留，未把它误称为 Iroh
寻址服务或公网可达保证。无需为此打开 IPv6 或修改路由器。

真实进程验收由 `crates/novovm-relay/tests/rendezvous_process.rs` 调用实际
binary，通过私有管道传入明确测试材料、在 loopback 上授权。双客户端并发
PUT/GET/ACK/重复 ACK/删除，连续两次启动和日志恢复、旧源回滚、错误发布者、
缺失恢复、未经显式许可、旧 V1 及未知身份拒绝、在线签名撤销均单独检查。
这些是工程进程验证，不是 Android Wi-Fi/蜂窝、抗封锁或匿名验收。

本次 Windows 结果：进程专项 4 通过、网络库 823 通过/11 原有忽略、默认
selector 7 通过、TLS-only 编译通过、network/relay all-targets 严格 Clippy
通过。进程专项与库测试层次不同，默认 selector 与全 feature 库测试有重叠，
不相加宣称独立测试数。日志见最新生产台账。

## 2026-10-13 自主策略 v2 运行入口

同一 `novovm-rendezvous-service` 增加本地自主策略，并复用原 V2 Offer/服务握手/寻址 RPC。
不要求 Publisher 或 24 小时测试运营许可。原 v1 配置继续按旧规则验证，不自动转换。

- 新节点：`--provision-self-owned CONFIG`，明确创建受 DPAPI 托管的节点和存储密钥并开始运行。
- 旧节点迁移：停止旧 owner 后，`--migrate-self-owned OLD_CONFIG NEW_CONFIG`。新配置的 root 必须是原 `service.journal` 所在目录，节点、客户端资源范围、Relay/DNS 及 record_id 保留。首次切换建立独立的稳定传输密钥；聊天/钱包密钥不作为载体密钥。
- 正常启动：`--foreground CONFIG`（持续运行）或原 `--service CONFIG`（SCM 停止处理保持）。不再传 EPOCHS，也没有隐式延长旧许可。
- 新配置字段：`version=2, root, allowed_clients, relays, nameserver, bind_address, relay_only, record_id`。`root` 为受限 ACL、无 reparse point 的绝对私有目录。批准后的配置与新传输 seed 由 `self-owned-policy.dpapi` 固定，不能直接改 JSON 扩权。
- `self-owned-record.json` 是当前自签定位声明，首次接纳必须新鲜并由接收方确认公钥；不是 Privacy 或全网信任证明。已有本地信任可用历史定位地址请求新签名租约，不可用旧租约继续业务。
- 监听端点持续存在；临时寻址存储 90 秒，每 60 秒切换；单个入站操作最多 30 秒。新请求选择实例和租约是原子步骤，旧实例随最后一个有界会话释放。原 TTL、去重与资源限制保持。
- 本轮进程测试覆盖新节点、显式旧节点迁移、两次实例交接及重启。手机结果单独记录，不等于匿名或已摆脱外部 Iroh Relay。
- 16 MiB 加密日志仍无安全轮转，到限停止；完整旧备份回滚仍需额外新鲜度机制。当前不承诺长期无人值守产品签收。

## 2026-10-14 当前用户托管节点自动启动

已有 v2 节点的 DPAPI 材料属于创建它的 Windows 用户。保持原身份的安装入口：

```powershell
./scripts/windows/install-novovm-rendezvous.ps1 -UserSession -Action Install `
  -SourceBinary <windowless-native-exe> -ExpectedSha256 <verified-hash> `
  -Configuration <existing-self-owned-v2-config>
./scripts/windows/install-novovm-rendezvous.ps1 -UserSession -Action Status
./scripts/windows/install-novovm-rendezvous.ps1 -UserSession -Action Uninstall
```

复用同一原生 `--foreground CONFIG` owner，安装到当前用户 LocalAppData/NOVOVM/
Rendezvous，配置的原 root 不变，不复制或重建密钥/日志/信箱。无原 custody/history、
错误哈希、其他任务所有者或不同安装配置均拒绝。原服务账号 v1 安装方式保持。

Windows 任务计划在该用户登录后启动；无口令存储、无提权、无 Powershell 常驻循环。
原生程序使用 Windows 无控制台子系统，任务直接持有进程而非无人监管的子进程。
同一任务不重叠，向系统配置最多重启三次、间隔一分钟；没有执行时长上限。
此为配置请求，不是恢复保证：本机实际异常退出测试在90秒内未自动重启，
四个独立退出码1的任务对照亦未恢复，详见生产台账。当前崩溃自恢复未签收。
安装只登记，不中断已运行的前台节点。首次接管须先确认原进程退出，再启动任务；
节点原文件锁继续拒绝第二写入者。Status 同时报告真实进程计数，任务 Running
和旧 status.json 都不独立证明网络在线。卸载停止该任务并移除自动启动，保留数据。

这是**登录后启动**，不是未登录即运行或睡眠期间在线承诺；不修改 VPN、路由、
网卡或防火墙。换程序路径后的入站防火墙适配、真实重启登录及长期恢复仍应单独验证。
停任务是系统进程终止，依赖现有日志/SQLite 崩溃恢复，不声称优雅连接收尾。
需不登录运行的设备仍走专用服务身份的明确密钥迁移，不能直接冒用当前用户 DPAPI。
