# 原生多机验收客户端

`novovm-native-client` 是原 `NativeApplicationClient` 的薄命令行消费者，不是新节点、
执行器、账户库或部署服务。网络只走原 NovoRUDP/APFL；无 HTTP/JSON-RPC fallback。
配置和标准输出使用 JSON 便于操作，不将 JSON 作为原生交易或查询 wire。

## 现有能力与本片缺口

- `native_pipeline/service/client.rs`：原 secure session、prepare_uca、query关联与限流。
- `novovm-protocol/uca_spend_batch.rs`：原 NUAPFL01 二进制结构批与规范 NUAS 签名。
- `novovm-protocol/native_query.rs`：原类型化查询、精确请求关联与公开结果类型。
- `native_product_uca_recovery.rs`：实际子进程、3/4确认、追赶、冷恢复和经济核对。

之前这些完整用户操作主要由测试夹具调用；overlay/Relay诊断不是原生主链客户端。
本片只补操作端消费，不新定义签名字节、权限或消息格式。不生成签名、grant或创世，
不读取数据库、不启动或停止节点、不触及手机持钥。App继续消费相同SDK。

## 构建与身份

各机器先在干净main上 `git pull --ff-only`，核对同一指定提交，然后：

```text
cargo build --locked --release -p novovm-node --bin novovm-node --bin novovm-native-client
cargo metadata --no-deps --format-version 1
```

程序在metadata的`target_directory/release`下，Windows增加`.exe`；不能猜另一个target
目录并误用旧产物。Linux现有打包脚本也包含该客户端。记录实际启动node、客户端和
实际加载AOEM的SHA-256、平台、提交及配置摘要；不要要求DLL与SO跨平台哈希相同。
本机编译不替代Linux安装、systemd或真实多机验收。

## 客户端配置

按 `service/client/operator.rs::ClientConfig` 准备本地配置，所有字段显式提供：

| 字段 | 来源 |
| --- | --- |
| `chain_id`, `genesis`, `protocol`, `validator_epoch`, `validators` | 同一经核对公测配置的公开域、最终创世承诺和验证者集合；hash/key为32字节数组 |
| `node` | `{ "public_key": [32字节], "address": "IP:UDP端口" }`，必须属于该集合 |
| `bind_addr` | 本机可用UDP绑定地址，例如`0.0.0.0:0`，IPv6需同族配置 |
| `transport_key_file` | 本机已有或专为本次隔离验收准备的非验证者传输用途密钥，严格32原始字节；相对配置目录解析 |
| `timeout_ms` | 明确1–30000毫秒，总操作期限，不自动重试未知交易结果 |

配置不是发现机制或外部信任来源：操作员必须从可信渠道核对节点公钥和链域。
不要分发验证者签票私钥、UCA管理密钥或ML-DSA资产私钥作为客户端密钥。
现有原生传输身份仍为Ed25519；NUAS资产签名仍为ML-DSA-65，二者职责不同。
这个工具没有完成全面PQ传输/共识迁接，不得据此激活正式主网。
本工具主动拒绝使用集合中的验证者密钥做客户端。

## 命令

```text
novovm-native-client client.json status
novovm-native-client client.json query request.nvquery
novovm-native-client client.json consensus-proposal HEIGHT ROUND TX_HASH
novovm-native-client client.json submit signed-batch.nuapfl
```

- `status` 使用原类型化Status，不需要额外文件。
- `query` 消费原SDK `native_query::encode(Frame::Request(...))` 的二进制文件。
  支持现有Receipt、UcaSpendState等选择器，不发明另一套JSON查询格式。文件里的旧
  session/sequence不会复用；原客户端重新生成关联。Response文件不可当作Request。
- `consensus-proposal` 使用原认证查询通道，按完整交易哈希查询服务端的当前高度和
  轮次。返回待处理池成员状态、该位置最后一条资产选批门状态、该交易是否在仍保留的
  本地候选批，以及原共识 owner 中匹配的本地候选值、body ID 和 canonical 签名提案。
  `Prepared` 表示出站消息已准备，`Admitted` 表示原传输接管，`Sent` 表示已写出
  全部初始分片，`Delivered` 表示收到至少一个对端的原认证传输完成确认。
  `Unobserved` 表示当前进程没有该提案的发送记录。候选和提案来自现有 owner，
  离开当前高度或轮次后 `observation` 为 `null`。这个查询不补写历史观测，
  不触发签名、投票、重发或最终确认。
- `submit` 只接受原 `UcaSpendBatch::encode()` 的NUAPFL01，里面须已有真实签名及
  原账户定位提示；不接受旧V3或JSON交易，不在CLI中补签/授权。接收节点仍独立验签
  和验权。CLI上限64行/512KiB是本地操作预算，不是主链协议容量或TPS上限。
- `transport_delivery_only` 不代表准入、执行或最终确认；`finalized=false`明确保留。
  超时/会话变化后业务结果未知，必须保留原始签名输入并查询，不自动重签或推进nonce。
- 查询输出是绑定该节点/域的 `node_observation`，不是独立QC/ZK证明。即使返回Finalized，
  四节点验收仍需逐台核对同一交易回执、根、余额、nonce、费用和预算，不能一台代签四台。
- 展示层的经济数值全部输出十进制字符串，保留完整u128，不经浮点数转换。Pending、
  Missing和真实零余额是不同状态；Pending需要稍后重新查询，不可解释为到账或无资产。
- 工具不写输入文件或删除数据。不要把配置/密钥/数据库上传到CI或GitHub证据目录。

## 验证者重连：待处理原生 NOV 交易补拉

补拉不是客户端命令：原 NOVONET 验证者在建立或更换认证会话后，走现有交易输入通道
请求一页待处理资产交易。服务端只响应本链已认证的配置内验证者，要求会话标识仍是
该对端的当前会话；每会话一次，每对端请求和响应间隔分别至少 10 秒，积压请求最多
保留 30 秒。每页最多检查 128 个入池位置、返回 16 行 NUAS 或 128 KiB 签名行数据，
继续受原事务批大小、广播和传输预算约束。空页不回包；这是有界补拉，不是全池复制。

回应使用原 `AssetTransactions`，复用已保留的签名行；接收端仍走原 AOEM 验签、资产
权限、nonce 和去重，不把对端接收或补拉完成当成执行、持久化或最终确认。旧版本无法
应答该请求，仅换接收端不能从旧版本取回交易；新旧版本原广播保持原样。

当前 A/B 上 `68d83d0b…9bd6cd4` 的待处理副本不随退出保存。部署前先独立核对 A/B
仍返回 `Received`；未获另行部署批准前不切换。获批后也只能先准备不持有该交易的
接收端（如 C），保持 A/B/D 在线。如果仍无已经支持补拉的在线持有端、可验证的持久化
回执或经批准的无损迁移方案，就停在这里，不滚动升级两台持有端。验收成功必须是同一
哈希在升级接收端真实入池或已最终确认，不能只看握手、请求或启动成功。此工作不重发
两笔交易、不更新 D，不将一次本地补拉测试计为四机最终性验收。

## 各设备现在可以做什么

用户已确认A本机、B阿里云、C手机调试笔记本、D Linux AI主机为候选，E/F手机不计入
BFT，G的MEV主机仅备用。各机Codex的可复制交接与分阶段退出条件见
[多机NUAS交接](NOVOVM_MULTI_HOST_NUAS_HANDOFF.md)。

先回传机器标签、OS、CPU/RAM/可用磁盘、UDP可达地址、现有服务/端口、代码提交、
是否已有独立测试数据目录与批准的公共创世配置；不要回传私钥。阿里云现有Mailbox、
Relay、Web服务不覆盖。本机负责共享客户端/经济路径，其他设备负责各自运行证据。

配置核对后按已有服务启动机制部署，不更改现有链域、不清库、不复制其他节点DB。
两个完整验证者在四者集合中不能产生三票确认；双机先验部署与连接，四机齐备后才
验交易/恢复。先少量真实签名转账、到账再消费，随后离线40高度、晚启动、追赶强杀、
原库追平及冷恢复；最后再单列65,536笔TPS/P95/P99，不能沿用Ed25519旧负载成绩。

目前所有节点均为公测；正式发行、正式分配、服务结算、移动运行库及多机性能分别验收。
