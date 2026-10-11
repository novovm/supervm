# 受保护来源镜像与邀请导出

2026-10-12，输入 `4e637a12`，认领 `468122df`。本片在既有 network V2
分发接口上增加 Windows `novovm-bootstrap-mirror` 运行 owner。它分发已批准
的签名来源，不持有发布者签名密钥、不签节点资格、不维护聊天目录。
当前为明确批准的工程连通路径，不能用于声明匿名；正式 Privacy 门禁不变。

## 调用链与源码

```text
独立 publisher → epoch-N.authority.json
→ rendezvous_owner / run_owned_rendezvous_epoch_v2
→ 激活并持久接受后 sources_changed → active.sources.json
→ bootstrap_mirror_owner / 原 ServiceLeaseBook + ServiceLeaseFile
→ 原 serve_bootstrap_sources_v2 / 完成双向流
→ 客户端原 refresh_bootstrap_sources_v1 / 签名校验 / 耐久缓存
→ 当前已授权节点 / 原身份握手 / PUT、GET、ACK
```

`crates/novovm-network/src/product_rendezvous_daemon.rs` 的观察回调仅在
激活/持久接受后输出，预备任期不能冒充已激活节点。撤销先接受再输出，随后
路由检查终止服务。`crates/novovm-relay/src/rendezvous_owner.rs` 原子写入
`active.sources.json`。文件中的签名证明授权，不能证明地址此刻仍在线：进程
退出可能留下尚有效的旧记录，客户端仍需处理不可达及重新刷新。

`crates/novovm-relay/src/bootstrap_mirror_owner.rs` 托管独立随机传输密钥与
存储密钥，使用原 DPAPI 包装及加密 CAS 日志。Iroh 的
`with_owned_service_identity` 仅为显式配置的服务保留传输公钥；普通客户端
与寻址任期仍使用原来的临时身份，不复用钱包、聊天、节点或发布者密钥。

## 配置、运行及信任交接

构建时设置本仓 `CARGO_TARGET_DIR`、`CARGO_BUILD_JOBS=1`：

```powershell
cargo build --locked -p novovm-relay --features iroh-transport `
  --bin novovm-bootstrap-publisher --bin novovm-rendezvous-service `
  --bin novovm-bootstrap-mirror
```

镜像配置使用严格 JSON，未知字段拒绝：

| 字段 | 要求 |
| --- | --- |
| `version` / `engineering_direct_allowed` | `1` / 明确 `true` |
| `state_directory` | 私有、独立、绝对目录，无 reparse point |
| `source_path` | 目录外的既有来源文件，指向节点 `active.sources.json` |
| `trust_anchors` / `minimum_signatures` | 已核对的 1–8 个唯一强公钥及有效阈值 |
| `source_ids` | 1–4 个明确来源 ID，输入必须精确覆盖这组来源 |
| `relays` / `dns` | 已批准中继至多 4 个，可为空；明确 DNS IP |
| `bind_address` / `relay_only` | 直接服务要求固定本地端口；仅中继模式禁止直接绑定且要求已批准 relay |
| `permission_expires_at_ms` | 明确绝对期限，首次登记距当前最多 24 小时 |

直接服务的固定本地端口仅用于同一入口重启，不是普通手机/弱节点的入网要求，
也不保证运营商 NAT 映射或公网地址不变。仅中继方式仍需可达且已批准的入口。

1. 沿发布者文档登记并启动独立发布者和节点，等待已激活来源。
2. 在最终运行的 Windows 身份下执行 `novovm-bootstrap-mirror --initialize CONFIG`。
   要求空目录；随机密钥与完整固定策略一起 DPAPI 托管，私钥不写入 JSON、参数
   或日志。CurrentUser DPAPI 不等于 TPM 不可导出密钥。
3. `--foreground CONFIG SECONDS` 运行 1–86,400 秒，或通过 SCM
   `--service CONFIG` 运行同一 owner；服务名 `NOVOVMBootstrapMirror`。
   `--check CONFIG` 只验证配置，不能作为密钥/服务在线检查。
4. `--configure-recovery` 只配置已有服务的两次崩溃恢复（15 / 60 秒），安全失败
   不无限重试。此片没有安装脚本，没有实际安装新 SCM 服务。
5. 将输出 `invitation.json` 经已认证的带外交接送给用户明确批准，交给 NOVOCHAT
   既有 `novovm-bootstrap-v1` 导入入口；不另建聊天协议。`peer.json` 仅包含
   镜像公钥、地址和固定许可期限，不含私钥。

**签名只覆盖来源清单。** 邀请外层的镜像、公钥信任锚、DNS/relay 配置需要
完整的可信交接和首次核准，不能因为内层清单自带签名就信任陌生邀请，更不能
声称本片已经实现认证二维码分发。客户端将外层策略绑定进加密缓存，来源刷新
不能替换它。源过期后遗留的旧邀请仍被客户端拒绝，不自动延长有效期。

## 运行与恢复边界

- 原载体作用域上限保持，每 110 秒内交接清理、用同一托管公钥重新绑定；只在
  原许可期内运行。重启恢复同一公钥/固定地址的验证仅覆盖本机 loopback。
- 来源最多 64 KiB，导出邀请最多 8 KiB；每镜像最多 4 个并发请求、每秒最多
  接受 16 次，每次 3 秒且不越过源有效期。状态只在变化时写盘，不记录通信内容。
- 100 ms 来源检查；更新先持久化再供给，清理旧的进行中请求。合法空来源作为
  撤销继续分发；过期源不返回。状态分别为 `serving`、`revoked`、`waiting_sources`。
- 伪造/倒退/等序号改写/配置变化/密钥或日志丢失均停止；缺失状态不自动登记。
  恢复持久历史后，即使把公开来源文件换回旧签名，也不能复活已撤销记录。
- 正常停止为 `stopped`，运行错误为 `failed_closed`；SCM Running 只表示进程状态，
  不证明节点可达、消息已送达或匿名合格。初始化/打开阶段失败不保证写出状态文件。
- 当前**未实现镜像许可或地址的自动续权**；24 小时上限保留，不能编辑配置绕过。
  此片是持续运行接口与有限授权期内的进程验证，不是已经可永久自动入网的产品。
- 未验不同服务账号隔离、Windows 重启、24 小时运行、公网和第三方 relay。
  16 MiB 加密日志安全轮转、整份有效旧备份的外部新鲜度依据仍未完成。

## 可复现验证与失败记录

```powershell
cargo test --locked -p novovm-relay --features iroh-transport `
  --test bootstrap_mirror_process --test bootstrap_publisher_process `
  --test rendezvous_owner_process -- --test-threads=1
cargo test --locked -p novovm-network --features iroh-transport `
  --lib iroh_transport::tests -- --test-threads=1
cargo clippy --locked -p novovm-network -p novovm-relay `
  --features iroh-transport --all-targets -- -D warnings
```

镜像真实进程专项 **4 通过**，发布者 **5**、节点 **5** 回归通过，原载体 **20**
通过，严格 Clippy 通过。真实发布者、节点、镜像使用独立随机托管密钥；测试
父进程不代签来源。覆盖两客户端、镜像强制退出后原邀请仍可用、旧源过期后
观察到已激活新源再认证并执行 RPC、在线撤销/重启拒绝旧源、期限/策略变化、
伪造输入/缺失密钥拒绝。节点回归增加“预备但未批准任期不发布”检查。

原始日志：`artifacts/mirror-final.log`、`mirror-process-final.log`、
`mirror-carrier-regression.log`、`mirror-clippy.log`。早期一次真实测试在任期
切换时对已退出但签名仍有效的地址拨号超时，见 `mirror-failure-diagnosis.log`。
测试改为等待镜像/客户端观察已激活的新代再进行 RPC；这是明确的同步条件，
**不是产品自动重连已通过**。全自动跨代重试仍需端到端验证。

NOVOCHAT 的实际导出邀请导入/加密缓存恢复结果单列在其 Application 台账。
本片没有正式 APK 安装、手机身份导入或 Wi-Fi/蜂窝 E2EE 消息验收，Privacy
资格、全网 NAT 成功率和抗封锁能力均未由这些本机测试证明。
