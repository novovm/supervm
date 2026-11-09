# OpenSSL安全依赖修正与原网络观测候选

## 2026-11-10 后继交付：68fa7588已在B→A原库运行

固定源码`68fa758820889c3f5e8bb230aa42cef100b77b4b`的原自动
[CI 42685932056](https://github.com/novovm/supervm/actions/runs/42685932056)
已completed/success。本轮没有手动重跑或新增workflow dispatch。
旧55源码运行最终failure，安全作业失败；不能改写其结果。

两平台实际包内身份/NAT各7/7、原打包器/全部成员哈希与AOEM资格通过。
包及node/client/ctl/AOEM固定摘要见[候选清单](candidate-68fa7588/candidate-manifest.json)，
[包内原始证据](candidate-68fa7588/original-package-evidence.zip)SHA256为
`58f857cb788702d39667ad8271ab9f06a9f110808cb3828d16b5a8f56ab2baa4`。
候选清单的deployed=false是封包时状态；后继部署以本节及更新回执为准。

按原更新器先B后A，以existing打开同一原库，未初始化、换钥、改共识或资产规则。
旧进程精确终止，不冒称正常排空：B原PID2777931/SIGTERM→2858350；
A原PID25216→37496。B仍novovm/UID996，A仍LocalService/S-1-5-19。
两端AOEM保持原固定库，实际装载路径/哈希、监听41050/42050及41051/42051、
受保护材料哈希和执行健康通过。A仅复制同范围新程序规则，三类防火墙保持启用。
原库路径、全部摘要、身份保留和认证查询见[更新回执](candidate-68fa7588/update-result.json)。

| 主机 | 换包前/后认证Status | 原状态根 | locator新鲜度与运行推进 |
| --- | --- | --- | --- |
| B | 58ms / 56ms | f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333 | 序号4523→4525，ticks推进，execution_failures=0 |
| A | 40ms / 28ms | 同上 | 序号5453→5454，ticks推进，execution_failures=0 |

B邮箱、Relay、nginx、Docker前后状态和PID本轮相同；没有为了此目的保留旧节点包。
未执行回滚；C/D未操作。仍是原24小时内部运行窗口，不宣称永久SCM服务或持续稳定性。
[原始更新及双端观察](candidate-68fa7588/original-update-evidence.zip)SHA256：
`99be3b41c83227a6b0da6dfef949c037d7afaf54345fc42ab4e486f05aa68828`；
归档全成员大小/哈希已独立校验。首次采集B两次locator落在同一40秒发布周期，
采集器拒绝“序号推进”结论；原样保留后追加final2，未重启节点或修改周期。

### 已定位的失败边界，不是完整根因

A新进程首次至第16次记录一致：`discovery_lookup`，
`discovery deadline at carrier connect: deadline has elapsed`；B该观察累计0。
源码`service/network/discovery.rs::query`先调用`IrohScopeV1::connect`，返回后才
写入口、做NOVONET bound认证、链域选择、发布和查询。因此本次失败在发现用Carrier
建立连接/流的阶段；不是已证明的ML-DSA拒绝、发现RPC拒绝或NovoRUDP对象DONE失败。
connect内部含地址/QUIC/流准备，本次记录不能再细分到其中哪一步，不推定UDP包丢失。

A唯一seed为B；只读对照旧seed与B当前签名locator，传输公钥、carrier endpoint、
direct地址（含47.103.59.206:42050）及relay集合相同，序号12对4523。
原self-owned策略允许历史坐标用于新的固定身份握手，不能仅以seed过期归因。
发现当前走原Iroh共享控制入口；这不表示对象主路径已改为Iroh，原生NovoRUDP优先不变。

本轮未发新测试对象，未重复四机配对；旧3fa的A→B原生Object DONE仍是旧版本证据，
不替68的新对象/多跳验收。下一范围是原Carrier connect内部与同连接双端证据，
不调大超时、连接数或资源上限，不先判定Meta/防火墙/密钥错误；未修复网络根因。

## 以下为源码修正时历史记录

输入源码：`55f947c9344bcdff0d7c17288439acd35df4ff38`。
原自动CI [42683324372](https://github.com/novovm/supervm/actions/runs/42683324372)
的Rust security job `129383748040`在根Cargo audit失败；其他作业当时仍运行。
不是SQLite回退，也不是EVM、ML-DSA或NovoRUDP测试失败。

## 原因与最小修改

原锁文件含`openssl 0.10.76`，RustSec `RUSTSEC-2026-0463`指出安全函数
`aes::unwrap_key`的输出长度断言反向，可能允许底层C函数写出缓冲区；修补版本
为`>=0.10.78`。本地已读原公告及CI原日志，不忽略该公告、不改安全门禁。

实际Linux依赖链为`lettre 0.11.22 -> native-tls 0.2.18 -> openssl`，
由原node及rollout-policy邮件消费者引入。仓库crates未找到直接调用unwrap_key，
但这不等于已证明所有传递调用不可达，也不作为忽略公告的依据。

用Cargo更新根锁文件，仅两包发生变化：

- `openssl 0.10.76 -> 0.10.78`；
- `openssl-sys 0.9.112 -> 0.9.118`。

runtime锁文件没有openssl/native-tls，不做无关更新。没有修改AOEM、SQLite、
RocksDB、身份、网络协议、资产规则、资源限制或超时。新程序继承第30节的
有界失败阶段观察；依赖修正不能解释现场尚未归因的发现失败。

## 本地结果

根/runtime Cargo audit均退出0，均为0个vulnerability；既有informational/yanked
警告保留，不表述为零警告。根/runtime Cargo deny均退出0；未新增ignore或修改策略。
Linux实际node及rollout-policy消费者严格Clippy通过。
Windows/Linux release原network owner各8/8（234.34s/229.99s）、discovery各2/2
及node严格Clippy通过。两平台rollout-policy现有测试各1/1通过，bin/doc的零项
不另算通过用例；该用例验证原HTTP错误回执，不冒充真实SMTP TLS握手验收。

原始记录在`target/openssl-security-20261110/`及
`target/novonet-failure-observation-20261110/*-openssl-final-*.log`。
已归档[原始输出与复现入口](original-evidence.zip)，全成员大小及SHA256独立核验；
归档和锁文件哈希见[evidence-manifest.json](evidence-manifest.json)。
制品构建继续复用原打包器、包内资格检查及固定AOEM，而非复用旧node二进制。
本记录不代表新候选包、云端完整CI或A/B部署通过。

## 现场与下一步

A/B仍为3fa程序；已取得的真实A→B原生Object DONE证据继续有效，见共享交接第30节。
完成本地受影响验证后固定新源码，制作两平台包，检查实际包内程序/库，再按原工具
更新B、核验后更新A。原库existing最快且能检验恢复，本轮无须清库或换钥。
用户开发期停服授权已明确，不重复申请；C/D本轮不操作。
新观察部署后依据实际失败stage定位，不把累计失败数或空闲bound routes=0当作根因。

这是内部开发推进，不是公开发布、四机中继/多跳、资产业务或性能签收。
