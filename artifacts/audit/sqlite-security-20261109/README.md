# SQLite安全依赖修正（源码/本地验证，不是节点换包）

> 2026-11-10更新：固定源码`3fa11f94`的原完整CI及Windows/Linux候选已完成，
> 运行节点尚未更新。当前交付、额外并行失败分类及existing方案见
> [3fa候选收口](candidate-3fa11f94-20261110/README.md)。以下保留首次源码修复时的结果，
> 其中“CI/候选尚未签收”是当时状态，不再作为当前等待项。

基线main `4ca654cfaf7062529d4a6e289e5ebfb79cd9b972`。
该提交自动CI42600865983的[Rust security job129104943856](https://github.com/novovm/supervm/actions/runs/42600865983/job/129104943856)实际失败：根Cargo.lock
命中libsqlite3-sys0.38.2 / RUSTSEC-2026-0459，exit1，后继runtime审计和Cargo deny
未执行，不能记通过。原2b97 CI42567619087安全检查成功时载入1422条公告；本次1429条，
期间锁文件未变。旧成功与新安全失败分别保留，不以旧CI覆盖新公告。

## 存储所有者与最小改动

主链存储没有改为SQLite。原node保留rocksdb0.22；原CandidateStore仍经
StorageSession打开AOEM库，CreateNew/Existing域与格式保护、原账本/状态所有者不变。
SQLite用于唯一NOVONET的L3/L4 MailboxStore及novovm-storage-budget的应用目录quota VFS，
不是另一套NOV账本。schema3及原既有迁移/事务/权限/回执规则未改。

按[官方公告](https://github.com/RustSec/advisory-db/blob/main/crates/libsqlite3-sys/RUSTSEC-2026-0459.md)
及[rusqlite0.41发布](https://github.com/rusqlite/rusqlite/releases/tag/v0.41.0)，协调修改两个
消费者的直接精确约束：libsqlite3-sys=0.39.1、rusqlite=0.41.0，继续bundled。
Cargo生成锁文件只更新这两个包的版本/checksum，没有全量依赖刷新，也没有手改生成锁文件。
依赖树仅一份SQLite绑定；quota C继续由原build.rs使用DEP_SQLITE3_INCLUDE的配套头文件构建。
RocksDB/AOEM、网络协议、密码/身份、共识和费用均未变，不新增数据库迁移。

新增单项storage-budget回归实际执行SQL sqlite_version()，与链接API版本交叉核对，
要求SQLite>=3.54.0。它是实际链接版本门禁，不冒充恶意DB/WAL漏洞利用的复现。

## 本地安全检查

沿原工作流四条命令分别执行：根cargo audit、runtime/Cargo.lock audit、根cargo deny
advisories/bans/licenses/sources、runtime相同deny。均exit0。
根audit载入1429公告、570依赖、漏洞0、ignore=[]；原paste/rand/crypto-bigint提示如实保留，
没有新增忽略、关闭检查或改公告库；runtime锁文件和原deny策略未改。
本次数据库commit899077d9d471b9fd3dbf4b2faea8ef26e83692e8。

## 验证口径与已保留的负样本

所有构建/测试使用当前新依赖源码、locked、原Rust1.94及原平台release算术配置。
不拿旧包中的node程序代替新代码；AOEM真实恢复测试复用未改、已固定的原库和manifest，
其库路径不代表旧包覆盖了新依赖。Windows在C原编译缓存构建，Linux用原WSL缓存；
不清理用户原库或证据，不接触实际A/B/C/D运行实例。

首次Linux默认并行network lib结果935通过/3失败/13忽略，失败在EVM选路原因和两项
RLPx跟进时限。只做一次已提交4ca旧依赖源码的同类对照：938通过/0失败/13忽略；
候选串行诊断938通过/0失败/13忽略。该对照不证明三项并行失败根因已定位或修复，
不能覆盖原负样本，也未改对应生产代码、断言或超时。

额外Windows全feature并行lib试跑1297通过/26失败/13忽略，包含carrier endpoint limit
及全局额度计数互扰。原Iroh ENDPOINT_LIMIT为4，既有CI对相关真实IO模块显式
--test-threads=1；不为并行试跑放大额度。按原隔离方式单独验SQLite信箱/运行时路径，
不能把上述并行全组改写为通过。
Linux同类额外并行组1293通过/27失败/13忽略，也保留原始失败；包含额度、计数和时限
反例，不断言27项均由同一原因导致，不签收无限制全并行network组。

最初node-store-guards过滤器零命中，仅完成编译、未执行用例，不计通过；后继以实际
native_pipeline::persistence::store::guard_tests模块核对非零执行。原零命中日志保留。
原始记录位于A的C:\Users\leadb\AppData\Local\NOVOVM\security-sqlite-20261109。
临时旧源码对照由git archive派生到隔离/tmp目录，无新分支、生产兼容路径或部署动作。

## 最终所列本地结果

| 检查范围 | Windows x86_64 | Linux x86_64 |
| --- | --- | --- |
| 原storage-budget全部单元（含新版本门禁、quota/WAL/失败原子性/重开） | 12通过 | 10通过 |
| MailboxStore资源政策与失败恢复 | 8通过 | 8通过 |
| 真实SQLite MailboxStore集成（含原schema迁移/TTL/领取/删除/事务失败） | 12通过 | 12通过 |
| 真实localhost Iroh信箱运行时，原串行隔离 | 6通过 | 6通过 |
| 原native mailbox session，原串行隔离 | 12通过 | 12通过 |
| 实际原库打开/域/格式保护模块（非零命中） | 6通过 | 6通过 |
| 真实AOEM启动三承诺、旧资产拒绝及existing重开 | 1通过 | 1通过 |
| 真实AOEM比例费用再花/原库冷恢复 | 1通过 | 1通过 |
| storage-budget严格Clippy，all-targets，-D warnings | 通过 | 通过 |
| network严格Clippy，all-targets/all-features，-D warnings | 通过 | 通过 |

格式及git diff检查通过。各组存在覆盖重叠，不相加为独立用例总量；额外默认/全feature
并行组、旧依赖对照、串行诊断、零命中均按前节分别记账。SQL事务失败/关闭重开不
冒充SQLite进程强杀，也没有新增恶意DB/WAL利用复现。主链恢复沿原真实AOEM/临时
RocksDB路径，不以SQLite或内存替身冒充；组件IO不代表四机实网。

[结构化结果](local-verification.json)绑定实际源码文件哈希、四项安全检查、两平台各组
非零用例数与独立失败样本。[原始结果ZIP](original-local-results.zip)共68项、211828字节，
SHA256 `3de4cc2dc2cf854a7a0cc2a411de795f347b0291f9569fab1f139881214d6bb7`。
包含实际stdout/stderr、原CI安全失败、原编排脚本及对照结果；无数据库、journal、
用途钥、私钥/受保护材料正文或候选二进制。原字节以ZIP为准，文本Git换行不替代它。

本轮没有自动更新或重启A/B/C/D、初始化实际链状态、更换用途材料或打开第三方资金业务。
源代码修复不让运行中2b97等旧二进制自动获得新版SQLite；新候选与受控换包另行固定。
本次仅签收所列源码修正/本地范围，不宣称完整云端CI、全部并行回归、候选包或部署通过。
