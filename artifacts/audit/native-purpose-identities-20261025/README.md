# 原节点用途分离与离线准备

输入 `424be4953d0e178f05cec4e4186fda2d7509d512`；实际交付源码为包含本报告的提交。
本机 Windows，单槽构建、单机进程测试。未部署 A–D、未生成真实部署密钥、未操作 B，
未激活新公测链或正式主网；所有运行状态均为隔离回归夹具。

## 复用与实际变更

- 原 GenesisConfig / UcaGenesisManifest 同时承诺独立签票与传输公钥映射，原协议 pin
  显式加入 transport-binding-v1；完整双射、两用途全集互斥、弱钥拒绝。
- 原 NativeChannel 接原 UdpTransport/endpoint，原 NativeLifecycle 从受信绑定映射身份。
  journal、投票/QC验签、3/4门槛、独立执行和发布权威不变，不引入另一网络或账本。
- 原 service 在加载 AOEM/账本/journal 前核对本机钥匙、用途、域及全部远端。
  operator 客户端使用共同表寻找传输公钥，不得使用节点签票/传输钥匙作为客户端。
- 原 Windows DPAPI / Linux protected credential 与加密文件 owner 提供密钥保护。
  原 novovmctl 增 generate/inspect；创建只create-new，inspect只公开返回，失败不转raw。
- 原文件 owner 从 Iroh-gated service 模块解耦为单一无 carrier 模块；原服务 API 重导出
  保持调用兼容。原 open/create 不变，新 open_existing 不创建缺失锁。
- 原 Linux 包声明节点专用 owner credential、两份protected用途文件；原管理器验证
  加密credential路径/覆盖边界，但明确 custody_verified=false，不冒充运行账户解密资格。

准备步骤、版本兼容边界、每机分工与待批准材料详见
[唯一操作说明](../../../docs/NOVOVM_PURPOSE_IDENTITY_PREPARATION.md)。

## 已执行记录

1. 初次 check 的新增测试存在 signature Vec/array 类型差异，按原字段类型修正为Vec后
   `cargo check -p novovm-node -p novovmctl -p novovm-relay --tests --locked --offline` 通过。
2. 最终原 native_pipeline 回归：428 passed，0 failed，69 ignored（新增protected专项另显式跑）。
   未运行的专用测试不计入通过。
3. novovmctl：58单元＋21非ignored CLI通过，1项真实AOEM创世CLI默认ignored；
   其中4项新用途CLI实际使用Windows DPAPI，验证独立生成、只读、拒绝覆盖/用途/域/篡改。
   随后显式运行该真实AOEM创世CLI，1 passed/0 failed，规范批准与启动编译一致、伪造拒绝；
   见[独立运行日志](genesis-cli-actual.log)，不把默认ignored计入常规21项。
4. 原 service 真实AOEM启动：2通过（RawDevelopment、ProtectedV1各1），7种无效材料
   均不得开新库或修复已有库；原管理批准、三pin和冷恢复有效，测试钥匙字节未变化。
5. 原 service文件测试：初次旧模块filter命中0，**不计通过**。核实feature耦合后只移动
   原模块并保持旧API重导出；新 `product_service_file::tests` 实际7项通过，包含
   缺锁无创建、互斥、错误credential、压缩和各持久发布点子进程崩溃恢复。
6. relay library：Windows 2项通过（用途信封边界、原DPAPI篡改拒绝）。Linux平台专项
   尚未在本机运行，由D/远端Linux门禁验证，不从Windows外推。
7. 原打包器夹具：28项安全检查、构建失败/缺产物/错误feature/错误crate拒绝、11程序
   合成装配、独立protected节点配置与credential模板通过。不是Linux实编/安装。
8. 四个受影响crate全目标严格Clippy、workspace格式、diff检查通过；原relay的
   `iroh-transport,tls-transport --all-targets` 编译通过，旧服务仍复用同一文件owner。
   CI仅在原任务加入短用途/custody检查，先枚举确认新反例存在，不让0命中冒充通过。
   源码/文档diff检查无空白错误；原始cargo日志保留其输出末尾空行，不为通过diff检查改写日志。

### 四进程真实 NUAS：检查点一

[原始回执](uca-recovery-report.json)，[运行日志](nuas-process.log)。debug原node及原native-client，
固定随包AOEM；四个原进程、四个独立DB，不是四台机器。

- node SHA-256：`52530c2d4fc7b6d4dcb6faa9398ab32703bf6b2593ad03aea7166d6cb8d51dca`
- AOEM SHA-256：`31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`
- 健康3台在1台缺席下推进40高度；缺席节点135.595秒后启动，超过原握手窗口。
- 追赶到耐久高度1时强杀，目标40归档尚不存在；原库重启后追平，未复制库/钱包补灌。
- 收款方真实ML-DSA-65再次消费，最终4台同高41、相同根/回执；费用、授权预算、nonce、
  原签票outbox与冷恢复核对通过。总用时173.109秒，非TPS成绩。
- 此检查点早于最后通用文件模块解耦重建；不得把该二进制哈希冒充后续构建。

### 最终重建后的复验

[最终NUAS回执](final-uca-recovery-report.json)，[最终日志](final-nuas-process.log)，
[A/A/B回执](native-aab-report.json)，[A/A/B日志](aab.log)。同一最终node，未变更确认终点。

- node SHA-256：`6cadfaf5e10d66bd8c9cf3a8f32ac1286bc578f4c39d6ddfc5a0aff5a05a5a07`
- native-client SHA-256：`55fdfe5139160d518b20c525396f467e4612becd08ebdfe58fccbe0eb21414bb`
- AOEM SHA-256：`31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`
- NUAS 1 passed/0 failed；健康三节点推进40高度、135.423秒晚启动；追赶耐久高度1强杀，
  目标40归档不存在、已耐久QC为3签；原库重启追平，收款方再次消费，最终4方h41。
  原配置与signing outbox保留、四方账务/回执/冷恢复一致，总172.862秒。
- A/A/B 1 passed/0 failed，12.58秒；原诚实进程与真实签票、独立传输身份、换轮及原库重启。
- 最终源码的428项native_pipeline与默认/原iroh+tls特性编译、严格Clippy均已通过。
  两次进程结果分列，均非吞吐测量；没有重复统计为多机验收。

## 不能外推的结论

无 Linux 本机服务账户/credential实跑，无跨机网络、生产发布、全节点ML-DSA、
GPU/TPS或完整业务证明新增资格。真实共同chain_id、公钥、分配与批准尚待汇总；
不填fixture根、不提供虚构共同配置。主网经济批准和激活仍是单独授权。
旧文件夹具/日志保留，不提交私钥、完整配置、数据库或测试密钥。
