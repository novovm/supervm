# 独立签名发布者可执行入口

2026-10-11，输入 `03406bd1`，认领 `712d8f88`。本片将 Windows 寻址 owner
的授权从测试父进程接到 `novovm-bootstrap-publisher`。它是运营者选择的
部署签名方，不是 NOVOVM 全网官方审批中心；可以与节点在同一台电脑运行。
正式 Privacy、手机身份登记及匿名路由门禁不变。

## 真实调用链

```text
节点 --provision → DPAPI 节点密钥 / identity.json（仅公钥，无网络）
发布者 --initialize → 独立随机签名密钥 + 存储密钥 → Windows DPAPI
运营者核对两端公钥 → 发布者 --enroll-policy → 加密持久策略
节点 owner → epoch-N.proposal.json（节点签名，原子发布）
发布者 owner → 固定节点/记录 ID/期限/中继许可 → 原 ServiceLeaseBook 严格验证
  → 原 SignedBootstrapManifestV1 → 原 ServiceLeaseFile 加密 CAS / sync
  → epoch-N.authority.json（原 Vec<BootstrapSourceV1>，原子发布）
节点 owner → 原 ServiceLeaseBook / V2 / 认证 Finished → PUT / GET / ACK
```

发布者只读取本地受控交换目录，不新增线上协议、不拨号、不持有聊天记录或
钱包权限。寻址和签名清单格式完全复用 network；NOVOCHAT 不增加发行实现。
父测试进程只登记公钥和启动两个真实程序，不代签任何授权清单。

## 配置与一次性登记

构建时显式设置本仓 `CARGO_TARGET_DIR`、`CARGO_BUILD_JOBS=1`：

```powershell
cargo build --locked -p novovm-relay --features iroh-transport `
  --bin novovm-bootstrap-publisher --bin novovm-rendezvous-service
```

发布者配置只有以下字段，未知字段拒绝。实际路径、公钥及许可应由运营者
核对填写，不复制测试种子，不从未认证的网络响应自动采纳：

| 字段 | 契约 |
| --- | --- |
| `version` | `1` |
| `state_directory` | 发布者独立私有绝对目录，禁止 reparse point |
| `exchange_directory` | 节点 owner 的交换目录；与发布者目录不能相同或互相包含 |
| `publisher_public_key` | `--initialize` 输出的 32 字节公钥数组 |
| `node_public_key` | 节点 `--provision` 输出、运营者已核对的公钥 |
| `record_id` | 固定节点记录 ID，1–128 字节 |
| `manifest_id` | 固定来源 ID，1–128 字节 |
| `allowed_relays` | 预先批准的 Iroh relay URL，最多 4 个；可为空 |
| `max_grant_ms` | 5,000–110,000 毫秒，不能超过已批准的服务任期上限 |

操作顺序：

1. 准备发布者私有空目录，执行 `novovm-bootstrap-publisher --initialize <目录>`。
   只输出公开 `identity.json`，私钥不写 JSON、环境变量、命令行或日志。
2. 将该公钥纳入节点配置的既定信任策略；按已有节点安装契约提供
   `initialize.request`，执行 `novovm-rendezvous-service --provision <节点配置>`。
   这一步只托管身份、输出节点公钥，不打开载体、不签发服务资格。
3. 核对节点身份后完成上述发布者配置，执行
   `novovm-bootstrap-publisher --enroll-policy <发布者配置>`。策略只登记一次；
   后续启动必须逐字段一致，不接受自动换根、改节点或扩张许可。
4. 启动发布者 `--foreground <配置> <秒数>`（1–86,400 秒）及既有节点 owner。
   之后任期签名自动进行，无须人工逐轮签字。前台有界入口与 SCM 共用 owner。

发布者也有 `--check <配置>`、`--service <配置>`，SCM 名称
`NOVOVMBootstrapPublisher`；`--configure-recovery` 配置与节点相同的两次崩溃
恢复上限，明确报告的安全失败不触发崩溃重试。此片**没有发布者安装脚本，
没有安装或运行 SCM 服务**。必须在最终运行的 Windows 身份下初始化 DPAPI；
不能将普通用户的 DPAPI 文件复制给另一服务账号。正式安装仍须落实独立账号
及目录 ACL：节点不能读写发布者托管目录，发布者可读 proposal / 写 authority。
同一用户下的本机测试只证明独立密钥和进程，不证明 OS 账号隔离。

## 持久与撤销边界

- 当前一个发布者实例登记一个节点，候选披露上限 1；不是全网 IP 目录。
  只保留当前和预备两代申请/签名结果，目录扫描上限 64 项，最多两份申请。
- 发布者密钥与节点/传输密钥不同。CurrentUser DPAPI 复用现有封装；密钥在
  使用期间进入进程内存，不宣称 TPM 或不可导出密钥。
- 签名结果先在原加密 journal 中 `commit/sync`，再发布。重启重发完全相同的
  已签结果，不能同序号重签不同内容。已有节点时间、序号、发布者时间和策略
  持久保留；错误签名、超期、未批准 relay、倒退、等序号不同内容均失败关闭。
- `--revoke <配置>` 持久写入撤销请求；命令成功只代表请求落盘。在线 owner
  优先持久写入永久 tombstone，再用同一既有来源 ID 签发空候选清单，更新两代
  authority 文件。正常轮询 100 ms，节点沿用 250 ms 撤销检查；不是实时延迟保证。
  `status.json.revoked` 表示发布者 tombstone 已落盘，不表示所有消费者已收到。
- 删除请求文件或重启两端不能撤销 tombstone。即使在“tombstone 已提交、替代
  清单未提交”之间退出，重开也先恢复撤销，不能重发旧的有效授权。相同毫秒
  不发行不同清单；任期更新推迟到下一次轮询。
- 缺失密钥/日志、部分初始化、并发 owner、配置变化、文件错误均停止；没有
  自动重建或覆盖历史。失去发布者时，节点仍按原任期过期/退避，不沿用旧身份。
- 输出文件只能由这个已登记 owner 管理；不要同时运行另一脚本写同一来源，
  撤销走发布者入口。不同发布者的阈值合签/协调尚未接到此单签发行入口。

## 验证与未完成项

Windows 当前用户、loopback 直接 QUIC，真实二进制专项 5 项：提前自动授权
下一任期及两端重启后的 PUT/GET/ACK、在线撤销并删除请求后重启仍拒绝、
已提交结果精确重发/等序号篡改拒绝、伪造申请拒绝、策略替换/密钥缺失拒绝。
节点预登记没有开启载体或建立 service journal。普通 owner 回归 5 项通过。

发布者单元入口 3 项通过（含既有 DPAPI 验证），覆盖撤销提交中断恢复、TTL/
中继/时钟/缺日志门禁；节点单元入口 2 项通过。不同 binary 的 DPAPI 项重合，
不相加宣称 15 项独立安全性质。严格 relay all-targets Clippy 结果见交付台账。
原始日志在 `artifacts/publisher-{final-tests,provision-tests,clippy}.log`。

仍未完成：发布者/节点不同 Windows 服务账号部署、系统重启、24 小时运行、
16 MiB journal 安全轮转和整份有效旧备份的外部防回滚依据。签名来源已能供
节点自动取用；手机读取它的**常驻镜像入口、镜像端点续权和可信邀请发行**尚未
接上。这里不输出“最新节点地址即当前可用”的手机邀请，避免把预备任期
误当作已激活服务。正常 APP 两手机 Wi-Fi/蜂窝消息、无官方入口实验和匿名
路径尚未验收；没有部署测试根到手机，也没有使用公网/第三方 relay 验收。
