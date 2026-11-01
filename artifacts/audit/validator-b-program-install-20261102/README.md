# B Linux程序安装回执（未启链）

2026-11-02，`A-B-DEPLOY-20261102`。用户确认尚无共同配置，先安装程序。
A在既有WSL Ubuntu构建，沿原Linux打包器生成完整候选，在B阿里云Ubuntu x86_64隔离安装。
没有新建Git分支、替换正式随包库、启动验证者、生成创世/用途密钥或分配NOV。

## 固定制品

| 项目 | 实际身份 |
| --- | --- |
| SUPERVM源码 | main `14333419ee495e5d216761d31827271a252d9df5` |
| 原打包器 | `scripts/novovm-package-product-linux.ps1`，独立AOEM包根与核心SHA pin |
| 目标 | `x86_64-unknown-linux-gnu`，Rust1.94.0；不是D的ARM64包 |
| 程序 | 原node/client、6个product工具、rendezvous、supervm-relay及novovmctl，共11个 |
| 归档 | `novovm-14333419-linux-x86_64-20261102.tar.gz`，354,664,477字节 |
| 归档SHA256 | `7d4f4dc184300eb3e19081db8e5199bf32bc1d729fb6a81be0071bdbc339229a` |
| B安装目录 | `/opt/novovm/releases/novovm-14333419-linux-x86_64-20261102` |
| B接收目录 | `/home/ecs-user/novovm-incoming-20261102`，历史incoming未删除 |
| 核心SO | 160,288,664字节，`d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33` |
| node SHA256 | `56989b83c13d4eb77065cecd6537f30e1db0f2183fbe33036e5f65e7c3686f70` |
| client SHA256 | `5fade645df62278d08e530580d9fef78089733ecd516bca28ea23f26fec25d61` |

完整83项清单见[release-manifest.json](release-manifest.json)。A组包前后均核对洁净HEAD，
B核对独立归档pin、清单源SHA、长度与逐项哈希；安装后再次逐项通过。
本轮没有建立`/opt/novovm/current`、`/etc/novovm`或`/var/lib/novovm`。
版本目录root所有、目录0755、普通文件0644、bin程序及原安装脚本0755；没有运行该服务安装脚本。

## AOEM来源与范围

在独立目录解包AOEM `4c57372ad0cc12123df5ca9e163a130039bfb599`，叠加原候选
`artifacts/mldsa-native-paired-20261027/source-overlay.tar.gz`，SHA256
`8ee88c113611cbdcfacce949ae06b49f9870bb8cdd570a8eddeb8343505ab4fb`。
只在快照用Cargo将libz-sys1.1.23升级到1.1.31；最终Cargo.lock SHA256
`46625b6be8ad25c323fbba6e23263fe9f9f1b649805303ebdb89a7fe7f7abbec`。
没有改兄弟仓已有未提交工作。来源字段见[core-candidate-identity.json](core-candidate-identity.json)。

完整核心特性为默认`apfl-model-session,word256-gpu-forward-production`加
`rocksdb-persistence,wasmtime-runtime,privacy-verify,mldsa,zkvm-executor,risc0,sp1,halo2`。
构建保留告警，使用`--message-format=short`，没有跳过RISC0 guest或删掉特性。
不是CPU精简库；B仅验收显式CPU ML-DSA合同，不据此签GPU或证明后端运行。
其他sidecars保留原`1377cd1e`，没有重建或获得新资格；来源是混合组件，不能套用新核心的SHA/通过范围。

正式`aoem/linux/core/bin/libaoem_ffi.so`仍为
`4d06f36a67f41035661100ac0fd18c67dcae60bf2cdd5df28db33b555c538309`，未替换。
新核心只放在隔离候选包与B新版本目录。

## 实际验证

| 地点 | 检查 | 结果/边界 |
| --- | --- | --- |
| A Linux | 原novovm-exec单元回归 | 83通过/29忽略；不是实际SO验收的替代 |
| A Linux | 原aoem-bindings单元回归 | 33通过/8忽略 |
| A Windows/Linux | 原打包安全、构建失败、11程序装配及独立pin反例 | 通过；夹具不是实际ELF签收 |
| A新SO | `actual_semantic_mldsa_cpu_contract` | 1通过，覆盖44/65/87，坏签名/消息/context/公钥、行结果、并发及结果所有权 |
| A新SO | 原存储原子批写/有序multi-get/冷重开 | 1通过；非掉电或链恢复 |
| A新SO | 原存储线程owner/独立数据库/同路径排他 | 1通过 |
| A/B | 包83项逐文件验证 | 通过 |
| B | 11程序和新SO的`ldd` | 无缺失依赖 |
| B | 原novovmctl lifecycle帮助 | 正常，未执行lifecycle apply |
| B新SO | 同一个原novovm-exec测试二进制的实际统一语义CPU验签 | 1通过/0失败，1.31秒，不是TPS测量 |

B测试二进制放在incoming，不作为第二套产品或运行服务；SHA256
`082133e9bf80099ffd36e45ccbe29954f6c358bc977b399640124596009a2e18`。
运行时显式指定安装目录的新SO，只执行原`pq_signature::semantic::tests::actual_semantic_mldsa_cpu_contract`。
没有执行节点默认入口或任何示例创世。

原始输出：[实际核心资格](core-qualification.log)、[组包](package-build-windows-git.log)、
[A包校验](package-checksums.log)、[B安装权限修正后校验](b-install-permissions-verified.log)、
[B装载及实际验签](b-runtime-qualification.log)。

## 保留的失败与修正

- 旧Linux SO实际消费者测试失败：缺少新统一语义结果能力，不回退旧直接ABI。历史反例见
  [A构建过程回执](../../../docs/VALIDATOR_A_B_DEPLOYMENT_RECEIPT_2026-11-02.md)。
- Rust1.94.1和1.94.0的styled告警渲染均触发内部崩溃；[1.94.0回溯](kernel-ice-1940.log)
  在`annotate_snippets`渲染中。改用short后同一源码/特性完整release成功，不改MSM或关闭lint。
- WSL Git没有Windows系统LFS/CRLF配置，[原打包拒绝](package-build.log)保留。
  原打包器的四次Git状态/SHA检查改由本机既有Git for Windows执行；无洁净性豁免或Git配置修改。
- [首次安装](b-install.log)的复制保留了临时目录0700，归属root后普通用户不能进入。
  仅修正此次新目录和文件的权限，没有重写包或删目录；全部哈希复验及随后实际验签通过。
- 主线CI不是全绿：`fa39e603`及`14333419`对应运行被后续main推送的并发策略取消，
  不是通过；未主动调用CI取消接口。收口时`42787f3d`的运行仍在进行，不补记全套通过。

## B现有服务与下一步

安装前后邮箱PID944911、Relay958、Nginx3967403、Docker1414均保持active且PID不变。
没有修改它们的unit、配置、端口、防火墙、安全组、身份或数据。B没有启动novovm-node。

B的Codex下一步：只核对上述固定版本目录和哈希，保留原服务，不重编、不切current、不启链。
等待A提供经用户批准的共同链域、初始状态、用途公钥、未占用端口和原lifecycle plan。
推送本回执不等于B的Codex已读，也不替代将来的目标机激活回报。

本轮结束条件是**候选程序已安装且所列运行检查通过**，不是服务切换、正式发布、
四机连通性/故障恢复、经济/共识签收或性能恢复；没有产生新的TPS成绩。
