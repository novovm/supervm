# 原owner逐跳证据与固定候选（未部署）

2026-11-09后继：用户已批准B→A→C→D逐台固定883c原库existing更新，以及必要的
精确目标进程停止/原范围Windows程序规则绑定；不必重复索取同一授权。更新入口
本地回归已通过，B SSH未认证成功、候选自动CI尚未完成，因此本轮仍未更新任何节点。
具体准备、失败与执行边界见[交接第24节](../../../../docs/NOVORUDP_SHARED_CLIENT_HANDOFF.md)。
下方“另行批准”是候选冻结时的历史状态，由本段更新，不表示已实施。

程序源码固定为`883c6183a9dbaf73686e46381b178379bebf8217`。本轮完成源码、本地受影响
验证及Windows/Linux候选的独立离线核验，没有更新、重启或重新初始化A/B/C/D。
没有新增四机Object DONE、实网中继、Iroh切回、资产执行、最终性或TPS结果。
既有[A→C双端原生UDP验收](../a-c-native-20261109/two-ended-match.json)继续封存，不重发。

## 实际改动

原`NativeLifecycle/NativeChannel`的同一`NativeTransferDiagnosticsV1` Arc接到原
`network::Driver → network::relay → relay::hop`；默认无探针配置时不安装中继观察器。
未另写路由、传输、监听socket、后台服务、身份、队列、可靠回执或业务执行器。
原选路、NovoRUDP优先/Iroh备用、签名/AEAD、防重放、期限、带宽和连接额度均不改。

`native_network_probe.transport.relay_circuits`最多64条最新已认证circuit，记录：

- circuit/不可变route digest、原传输peer ID路径、本地角色/cursor、实际首跳binding；
  中继另记真正认证过的下游carrier binding，不从规划地址猜真实路径。
- origin/最终端在原E2E认证与同路径Finished完成后记录native Session；中继没有
  E2E会话认证权限。建路READY不代表对象已交付或该Session已被本地node安装。
- 每方向仅累计原bridge下游write_frame成功完成的opaque帧数/字节数。包括E2E握手
  和反馈，不能当业务对象数、物理链路总字节或计费凭证。返回写错另记；取消/读错
  不被算作成功。流drop时间不是DONE，也不擅自断言取消撤回了已发送字节。
- 原try_lock不等待；64条淘汰/遗漏及共享观察竞争显式计数。缺失记录不补成成功，
  无同步磁盘写入、原始业务载荷、私钥或exporter；无新的热路径JSON编码。

真实签收仍须两端同Object/nonce/身份/Session和实际Carrier binding，关联同一circuit，
再独立匹配各中继相同digest/path、真实上下游binding与两个方向的完成写入，最后核对
接收完整接管和发送原ACK/Object DONE。中继不能解密Object ID来制造“逐对象明文记录”。
Business Receipt和共识最终性仍由原业务所有者独立核验。

## 本地实际验证

最终原始日志使用`*-final-1-*`，不是把中间样本重复累计为更多独立用例。

| 范围 | Windows | Linux |
| --- | --- | --- |
| 有界中继观察、成功/失败写入、淘汰/缺失、竞争不阻塞 | 3通过 | 3通过 |
| 原逐跳上下文/身份/过期/重放/预算安全反例 | 3通过 | 3通过 |
| 原network真实IO组 | 7通过，233.73秒 | 7通过，233.34秒 |
| 原固定Probe/并发DONE/重复/错误接收者/取消/过期/关闭/配额 | 4通过，0.88秒 | 4通过，1.81秒 |
| network/node lib/tests默认及Iroh严格Clippy | 通过 | 通过 |
| 实际node bin/Iroh严格Clippy | 通过 | 通过 |

每平台实际执行17项定向测试，所有所列过滤器均非零命中。不是整个workspace或云端CI
已通过。原network组沿真实本机QUIC/原bridge，额外核对每个中间节点的实际下游binding、
双向完成写入、两端同Session/route digest，以及断开重认证后新circuit和旧流drop。
保留原61秒空闲/滚动额度窗口及原期限；233秒是包含多个反例的整组运行时间，不是
一次连接/重连延迟。这是组件crypto夹具及本机IO，不是四台物理机器或实际ML-DSA中继资产验收。
原Probe用例使用真AOEM存储及组件签名provider；包内实际ML-DSA验签资格另列如下。
两包的真实release构建也已通过；包内运行库和服务入口不是用Clippy代替的。

首轮`*-local-1-*`保留：network组Windows238.68秒、Linux233.20秒通过；途中补实际
下游binding及新旧circuit断言后，完成上表最终复验，不让首轮替后继源码背书。
fixture中不可达坐标/中继结束产生的failed/timeout日志是明确的反例输入，没有对
运行中节点实施这些动作。初次bin换行未满足fmt，格式化后两包fmt检查通过；WSL最初
调用不存在的/usr/bin/pwsh，在任何Linux测试执行前改为已有7.6.9安装路径，未改系统安装。

## 固定制品

A本地目录：`D:\WorksArea\SUPERVM\target\relay-evidence-20261109`。
大包不进入普通Git历史，不是已经复制、安装到其他机器。
完整node/client/ctl/AOEM及release manifest哈希见[候选清单](candidate-manifest.json)。

| 项目 | Windows x86_64 | Linux x86_64 |
| --- | --- | --- |
| 归档 | novonet-relay-evidence-883c6183-windows-x86_64.zip | novonet-relay-evidence-883c6183-linux-x86_64.tar.gz |
| 字节数 | 150333926 | 359230285 |
| SHA256 | 4f22ec10a87263947328c64c08a70fb0ca4b6e05eee2c3cfa31c8f1da62e57a4 | 3fa3b66645d99720e297c3688a704a601385c41ed36b20fefaca40420a87c91e |
| 实际node SHA256 | e02c4bb3020f348d33b87283fd6578d98db74ea523d96e32d86e60e2d2f7cbd0 | ee99594116dd2fcf3ea6eef93d61783abe700657b8cc77442e80e4dc2f05e5c9 |
| 原AOEM SHA256（未换库） | 1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50 | d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33 |
| 独立核验制品项 | 77 | 80 |

原打包器要求起止同一干净源码，按真实Cargo产物/feature组装，实际原库能力检查保持。
复制后又分别核验全量清单长度/哈希/路径/CHECKSUMS、实际加载库路径及标量/统一语义/
混合批ML-DSA验签、实际node的native_execution_tick/existing合同和377字节固定二进制
prepare/cancel。均无socket发送、用途钥生成、custody访问、DB创建/打开或服务启动。
Windows ZIP逐成员与已验文件核对；Linux tar实际diff无差异。见独立
[Windows](verified-windows.json)、[Linux](verified-linux.json)原结果。

## 原始证据

[原本地结果ZIP](original-local-results.zip)：26737字节、37项，SHA256
`224d390e34c0df1be56a2a2a37b94293cf0373b3402b367b01e4bbb3a5dd2b55`。
含两轮原日志、两平台构建/独立资格结果、候选清单及A本地验证/打包调用脚本；无
实际数据库、签票journal、用途材料或业务载荷。JSON外层Git文本可能规范化换行；
原始字节以ZIP成员为准，不把工作树CRLF/LF差别误写为内容篡改。

## 下一步与权限边界

当前A/C仍5181、B仍ddd、D仍082；本轮没有更新这些实例。新字段不能冒称现网已有。
源码及制品完成不等于本次完整云端CI、SCM实机或中继实网通过。推送按现有规则选
完整自动检查，不额外手动dispatch、改选择规则、skip、取消或重跑旧CI。

下一操作需要另行批准这份固定候选的**原库existing受控更新**。可按B→A→C→D逐台，
每台先核对真实PID/程序/原库/用途公钥/状态根，验证后才继续；必要时精确结束目标
node进程，保留各自当前程序/配置和回滚能力，不重启机器，不操作B其他应用服务。
Windows若确需新程序防火墙绑定，应只复制原核验过的完全同范围规则，不能禁用全局
防火墙。不得执行原082初始化任务；D的旧启动任务若含initialize，也不能用于回滚。
回滚仍是先停新实例，再以各自旧程序和**existing**打开同一原库，不回退账本、不删LOCK。
这段是操作范围说明，不是已经获准/已执行的更新脚本。

健康NovoRUDP不会主动绕中继；现有固定Probe没有--via开关。即使允许本次更新，也不
自动批准阻断直连、修改路径/防火墙/代理、伪造locator、重启中继或扩大额度。仅挡UDP
仍可能走Iroh直接载体/外部relay，不能算节点B/D逐跳。因此真实单/双中继须另有明确的
有界路径实验范围和上述独立记录，不能凭计划宣布完成。C/D当前不操作，不重清点、
换钥、初始化或重做已通过配对。旧C崩溃和历史恢复停滞根因仍未结案。
