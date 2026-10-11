# Windows 通讯节点托管与任期监督

**后续增量（2026-10-11）：** 已增加[独立发布者可执行入口](NOVOVM_BOOTSTRAP_PUBLISHER.md)，
用单独 DPAPI 身份和持久签名策略自动处理任期申请。节点 `--provision` 可在
不开启网络的情况下托管身份并输出公钥。真实两进程验证通过，正式服务账号
安装、手机分发及跨网仍未验收；下文保留此前节点 owner 交付边界。

2026-10-11；输入 `fde02bdf`，认领 `eb1e55d3`。本片是 SUPERVM 通讯入口的
平台 owner，不是第二套节点目录、聊天协议或金融节点。用户确认：目前没有
正式独立发布者，先完成实现与独立进程验证，不以此替代正式手机跨网验收。

## 授权是谁提供的

当前 ServiceLeaseBook 的发布者是客户端/运营者预先配置的信任公钥对应的
签名方。它确认节点与短期端点的绑定，不是官方审批所有设备加入 NOVOVM 的
许可。测试中的发布者只属于测试夹具，不是已部署的基础设施，也不能给
普通工程路径授予匿名资格。密钥独立不要求物理上必须再开一台电脑。

设备自行生成身份、参与发现/连接，与其他设备是否接受它的中继/存储/隐私
服务，是不同的决策。此次保持已有固定信任策略，不偷偷换根、自授发布者
资格或删除撤销历史。真正无官方审批的服务发现仍须完成相应邀请/信任策略
与客户端接线，不能仅删除签名验证来实现。

## 真实调用链

```text
Windows SCM / 有界 foreground 验证
  → novovm-rendezvous-service → rendezvous_owner::Owner
  → Windows CurrentUser DPAPI → 独立节点签名种子 + 存储密钥
  → owner.journal（原 ServiceLeaseFile，加密/文件锁/CAS/sync）
  → 预留本任期及全部可能在线租约的序号，先保存再签名
  → run_owned_rendezvous_epoch_v2（原 rendezvous-v2 服务实现）
  → 新 Iroh 身份/实例 + 节点自签 proposal
  → 外部签名源 → 固定发布者验证 → 准备就绪
  → 终止旧实例 → 重读新授权 → 恢复 service.journal/撤销/序号
  → 原 ServiceLeaseHost / V2 前导 / Finished / 不透明寻址 RPC
```

正常配置的进程生命周期不设任期次数上限；网络实例仍严格有期限。提前量为
5–30 秒，任期至少为提前量两倍且不超过原 110 秒。最多同时准备一个后继，
后继准备期不能提供寻址。外部授权验证成功后才交接；交接后再次读授权文件
并核对持久历史。UDP 使用独立分配端口，固定端口配置在启动前拒绝，避免两
个实例争抢同一监听端口。单实例仍最多 64 次接入/4 并发。

这不是无缝迁移：旧流在交接时结束，寻址记录仍是短期数据，客户端须重新
发现/发布。下一期来源如何自动分发给 Android 缓存，尚需发行/分发 owner
接线，不能把服务端换任期成功当作手机已经找到新端点。

## 系统密钥与数据目录

- 正式安装使用 `NT SERVICE\NOVOVMRendezvous` 虚拟服务账号；首次运行在
  该身份内生成随机材料，以 **CurrentUser DPAPI** 密封二进制 `keys.dpapi`。
  不用 `LOCAL_MACHINE`，不在命令行、JSON、日志或 stdin 上传递私钥。
- 节点签名密钥与存储密钥独立；私钥仍会短暂进入签名进程内存，这不是 TPM
  不可导出密钥、后量子托管或对本机管理员失陷的防护声明。
- `%ProgramFiles%\NOVOVM\Rendezvous` 保存程序；`%ProgramData%\NOVOVM\Rendezvous`
  保存管理员配置；其 `state` 子目录只给管理员、SYSTEM 和本服务身份访问。
  服务只能读程序/配置，能修改自身状态。拒绝托管路径 reparse point。
- `initialize.request` 仅由显式安装创建；首次先写 `enrollment.started`。
  密钥、owner 日志或激活后的 service 日志缺失/损坏，不自动重建。首次激活
  落盘中断也停止并保留现场，不通过“重新注册”恢复旧身份。
- 两个日志都复用原加密日志；序号预约先持久化，崩溃/失败申请也消耗预约。
  只清理已预约旧任期的明确 proposal/authority 文件，绝不清理密钥或日志。
  正常退出仅移除当前两代临时交换文件，状态报告只保留最新汇总。

系统 API 依据：[Microsoft CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)。
SCM 接线使用 [windows-service 0.8.1](https://docs.rs/windows-service/0.8.1/windows_service/)。

## 安装与运行契约

构建设置本仓 `CARGO_TARGET_DIR`、`CARGO_BUILD_JOBS=1`，执行：

```powershell
cargo build --locked -p novovm-relay --features iroh-transport --bin novovm-rendezvous-service
```

配置为 `{ version:1, prepare_before_ms, epoch:<原 RendezvousDaemonConfigV2> }`。
所有公钥、客户端许可及 relay 来源必须来自实际批准配置，不能拷贝测试种子。
owner 配置中 `epoch.enroll=false`、`grant_sequence=1` 只是明确禁用手工复位；
实际首次激活和序号由持久 owner 管理。安装器将 journal/source 基础路径固定
在自己的 state 目录。配置有未知字段或与已安装信任配置不一致时拒绝升级。

```powershell
# 先核验构建产物 SHA-256；真实配置和管理员身份准备好后使用。
.\scripts\windows\install-novovm-rendezvous.ps1 -Action Install `
  -SourceBinary <已核验的绝对路径> -ExpectedSha256 <核验值> -Configuration <真实配置路径>
.\scripts\windows\install-novovm-rendezvous.ps1 -Action Status
```

Install 设置 SCM 自动启动及崩溃恢复：15 秒、60 秒后各重试一次，第三次停止，
计数 24 小时重置。明确报告的授权/存储错误不会触发 SCM 崩溃重试。缺少发布者
回复是唯一可退避重试的授权失败：常驻进程保留，结束过期实例后等待，1 秒
起指数退避至 60 秒并加最多 25% 抖动，重新申请新身份。收到错误签名、过期
回复或撤销时停止，不能把这些错误归为“网络不佳”。Stop/Shutdown 取消网络
任务并释放文件锁。没有无限重启测试子进程的循环。

安装器拥有一条**仅绑定该程序的 UDP 入站**规则，不更改原
`NOVOVM-Relay-WSS-TCP-443-v1`，不关闭防火墙，不改 IPv6 或路由器。规则并不
保证共享地址能够接收入站流量。Uninstall 仅停/删本服务与该规则/程序，保留
密钥、配置、授权历史和所有权记录。

本地交换使用 `state/epoch-N.proposal.json` 与 `epoch-N.authority.json`，后者是
原 `Vec<BootstrapSourceV1>`，须完整原子发布。签名源发布进程不在服务密钥
owner 内；正式远端发布者接入/发布到手机尚未交付。SCM `Running` 只代表
进程存在；`status.json` 区分等待、候选已获授权、退避、停止与失败。

`--foreground <绝对配置路径> <1..100 任期>` 是同一 owner 的有界验证入口。
它使用当前测试用户的 DPAPI，不等于已验证虚拟服务账号/系统启动登录。

## 验证边界

实际进程专项覆盖：连续两任期与再次启动、稳定节点身份/变化的 Iroh 身份、
提前准备、原 V2 认证与 PUT/GET/ACK/重复 ACK、崩溃后序号不重用、旧授权
拒绝、错误发布者拒绝、密钥丢失不重建、在线撤销后即使得到新签名也不能
复活、发布者中断后不使用旧授权。常驻 owner 单测另覆盖缺少回复的实际
退避重试、停止以及重复 owner 不改写活跃状态；DPAPI 实际加解密/篡改检查。

本片没有安装 SCM 服务、触发 UAC、重启 Windows 或完成 24 小时运行；没有
手机消息任务，正式 APP Wi-Fi/蜂窝验收仍未完成。两部手机本轮只核对 USB
及路由，分别保持 Wi-Fi/蜂窝。全套六项产品验收不因上述局部证据自动打勾。

两份日志仍各有 **16 MiB** 上限且满后失败关闭，安全轮转和完整旧备份回滚
的外部新鲜度依据未解决。没有正式发布者/端点分发部署，没有自主匿名路径
验收。测试路径是本机直接 QUIC，未使用 NOVOVM 公网中继或第三方 Iroh
Relay，三者不能混算。默认 Privacy、能力位 0、NOV 交易后置不变。

最终测试数字、原始失败与修复记录见生产台账；不能把本片作为完整安全审计。
