# B Linux程序安装回执（未启链）

## 2026-11-06 当前：C/D已签收，四机共同初始承诺已核对

A已读取C `f791cfed`与D `54b59f6e`的原始公开报告ZIP，核对R2/程序/core pin、实际
LocalService退出码与生成/inspect公钥；A/B/C/D本轮独立服务用途材料齐全，八把公钥
各不相同。不要求C/D再次安装、换钥、重做六组认证。各机原始回执未改写。

原七用途清单（1亿NOV、个人七钥、S1本次不启用）及既定费用已构造为原初始化manifest，
8份身份/资产管理主体批准经原受保护`approve`生成。A的Windows和B已安装Linux9a
分别验证，签名域/初始root/最终commitment完全一致；完整值及原始文件位置见
[唯一共同配置当前增量](../../../docs/NOVOVM-FOUR-HOST-COMMON-CONFIG-20261102.md)。
没有私钥传输、库复制、签票、链状态初始化或节点启动，不称四机主链已运行。

原SCM仅`existing`，原工具缺离线建库入口；现已补`native-genesis initialize`，严格
复用原startup/pipeline/CreateNew/shutdown，不创建socket/controller/journal。两平台
各17项真实AOEM/custody/建库/原Existing冷开及反例、严格Clippy通过，原service单元
10项及node lib严格Clippy通过。新初始化源码/工具仍与已安装9a运行节点分别固定，
不以debug可执行文件操作真实状态，也不为此重新生成已合格用途钥。

实际B对端路由尚需公网可达地址：用户确认尚无端口映射，A已给A/C/D UDP41051/52/53
同端口映射计划，B仍41050。等待路由器型号/WAN核对，不将LAN地址假填给云端B，不
自动修改路由器/防火墙、开启DMZ、放宽来源绑定或回退WSS。其间继续固定原初始化工具。

## 2026-11-06 当前：A实际服务账户准备通过，C/D使用R2（本地集中交付记录）

用户执行恢复脚本后，A原固定程序77项哈希再次通过；没有重装程序。真实LocalService
（SID `S-1-5-19`）成功完成包内AOEM标量/统一语义/混合批验签资格、两把独立ML-DSA-65
validator/transport钥生成和原OS custody解封。生成与inspect公钥完全一致，A/B共四把
用途公钥各不相同。任务退出码0，原临时任务已注销（独立schtasks查询亦为不存在）。
权限夹具已恢复继承权限。原失败、空文件DACL审计均保留，不改写为成功。

这证明**A实际LocalService用途材料准备通过**，不是节点SCM生命周期通过：
没有安装节点服务、开链库、创世/余额初始化或启动节点，没有新TPS；七把资产钥未被
该操作读取或替换。B上轮安装及两用途解封结论保持，C/D本轮公开材料尚未收到。

程序仍为`9a6153f1eebda4da5c10829abf77575aa106ba8f`，二进制和release-manifest
不变。R2仅修正隐藏目录查询和文件/目录分别授权；A恢复实际执行的是同一安装脚本
中的修正ACL步骤，不是另一套权限实现。最终ZIP已逐流验证77项制品及4份元数据/
脚本，共81个非目录项；SDK嵌套CHECKSUMS仍匹配原已验独立pin。

| R2交付 | 独立核对值 |
| --- | --- |
| ZIP | `D:\WorksArea\SUPERVM\target\selfrun-delivery-20261106\selfrun-9a6153f1-windows-x86_64-r2.zip` |
| 大小 | 140,196,230字节 |
| ZIP SHA256 | `57bf8bc6fab08d5b7a85f4d15b2df30bbedcc1fa69d7ca361d3d3f99d63847eb` |
| 包内安装脚本 SHA256 | `3edee735b7c208b93a9a65b3a12da0744f8b613af50fa1e506ea5478d928deec` |
| 原Windows release-manifest SHA256 | `58c92085ca738c1e3e280960df9f16d5432ff38fabcf08fa1dca170b905eff5d` |

C/D各取得此R2 ZIP，核对后解到新目录，由管理员PowerShell运行包内同一脚本：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File "$unpacked\Install-Selfrun-Windows-20261106.ps1" -HostId C -PackageRoot "$unpacked\selfrun-9a6153f1-windows-x86_64"
```

C用`C`、D用`D`。只回传`c-`或`d-`的`provision-result.json`、`runtime.json`、
`validator-inspect.json`、`transport-inspect.json`，不传protected/credential/库/journal。
如已执行旧版而存在安装目录或失败报告，先回传原结果，不反复重跑、清目录或换钥；
A专用恢复脚本不能不改Host就交给C/D。没有短预约窗口，不重跑六组认证。
**旧ZIP和GitHub旧快照中的安装脚本不可继续使用，以此R2包内脚本为准。**
本轮安装修正、真实结果及R2记录先本地固定，待交付工作包集中推送，不额外启动完整CI。

公开证据：[A服务账户R2证据](selfrun-9a6153f1-A-custody-r2-public-evidence.zip)，
20,479字节，SHA256 `ed37705701b4643d57cce108511ac98949020f3a7e915e014b629740e07a0c76`；
13项明确公开文件，含两次失败、权限审计、实际成功/运行库/公开身份报告与原操作脚本，
不含任何受保护钥、包装credential或链状态。

### 历史：A真实安装问题及待提权窗口（已由上文后继成功结果覆盖）

A已由实际UAC提升执行安装：源及安装后77项哈希一致，固定程序目录已建立。
实测发现两个安装脚本问题，原失败均保留：

1. 隐藏的`C:\ProgramData`在Windows PowerShell5中须`Get-Item -Force`；原预检
   在任何复制/密钥操作前失败，已修正查询。
2. 原递归icacls授权未在普通文件留下有效ACE；实际审计中目录有三条Allow，但
   `bin/novovmctl.exe`的DACL为空，LocalService进程创建被拒绝。Task已实际运行在
   S-1-5-19，失败在runtime KAT进程启动之前，未生成validator/transport钥。

现已把安装脚本改为目录/文件分别创建明确DACL：SYSTEM/Administrators全权，
LocalService读/执行，文件ACE不带目录继承标志。**该修正尚未取得真实LocalService
成功结果**，不能标成服务资格通过。后续恢复UAC返回取消；不绕过提权，不重装或
删除原报告，不在桌面用户下代跑密钥准备。

之前的Windows ZIP中安装脚本已撤回使用；C/D暂不执行该旧ZIP脚本，二进制9a及
其77项清单没有改变。新脚本/交付归档待A真实解封通过后集中更新，不为此反复云端CI。
本地`target/selfrun-delivery-20261106/`保存`a-provision-result-before-hidden-path-fix.json`、
`a-provision-result.json`、`a-service-execution-audit.json`及有界恢复脚本
`Resume-A-ServiceKeys.ps1`。恢复只修该固定版本文件权限、重新执行已有失败任务、
核对原包哈希和实际SID、生成/inspect尚不存在的两把用途钥，成功后注销临时任务；
仍不安装节点服务、开库、创世或触碰七把资产钥。
额外的非提升ACL夹具尝试被权限拒绝，不计通过；恢复脚本只对明确的两项本地
`acl-fixture`测试对象恢复继承权限，不操作其他路径。历史失败/旧权限审计不销账。

## 2026-11-06 首次自营交付增量（当前状态）

最新直接授权：七用途金额合计1亿NOV，20bps/最低0.002/最高50 NOV、ceil、90/10及
余数归服务预算保持；本次暂不启用S1锁定/共管，七把资产控制钥由用户个人持有。
这不是公众资金治理完成，也不启用第三方资金/付费业务。

实际程序源码：`9a6153f1eebda4da5c10829abf77575aa106ba8f`。沿原打包器分别重建
Windows/Linux十个程序，复用逐文件核验的既有AOEM，未替换正式SDK。
新增的`native-genesis approve`只把原受保护用途钥接入原初始化审批/AOEM签名；
错域、错用途、错公钥、改清单/签名、跨主体重放及覆盖输出均拒绝。Windows16项、
Linux13常规+3真实库及两平台严格Clippy通过；没有把本地结果写成完整云端CI通过。
此前云端42153285988/9b2af049：Rust和原生恢复成功、两平台认证反例失败；
f1a7e0e0已本地完整修正并验证受影响作业，尚无后继完整云端成功结论。

| 制品 | 长度/独立pin |
| --- | --- |
| Windows ZIP | `selfrun-9a6153f1-windows-x86_64.zip`，140,195,880字节，SHA256 `5a946c364fca767e7805081bd81c538b637da72b29475daf4a11c40175ee1292` |
| Windows release-manifest | `58c92085ca738c1e3e280960df9f16d5432ff38fabcf08fa1dca170b905eff5d`；77项原清单文件 |
| Linux TAR.GZ | `selfrun-9a6153f1-linux-x86_64.tar.gz`，349,871,858字节，SHA256 `8bc21561725dcb9bb55f8e996e3374850404514b81c78263a3be49af68895159` |
| Linux release-manifest | `efad60ae58128fb5faa971bf09a93736c78fc9f02989d3e9f969bd2a3245fadd`；80项原清单文件 |

两个归档均在A的`D:\WorksArea\SUPERVM\target\selfrun-delivery-20261106\`。
它们不是Git仓库中的二进制；仅`git pull`不会取得程序包。Windows ZIP内含固定程序目录
和[一次安装/服务钥准备脚本](Install-Selfrun-Windows-20261106.ps1)，脚本SHA256为
`27e44c439a329fdb116bc262e3efb9a81bfa579cfedc0fdcbd280f59e2f04046`。

### B已实际完成

- 新版本安装在`/opt/novovm/releases/selfrun-9a6153f1-linux-x86_64`，独立归档/manifest
  pin、源码SHA和80项文件哈希全部核对；未切换旧版本目录或已有应用入口。
- 新建原节点服务账户`novovm`，沿已有systemd加密credential及原`native-identity`
  生成/inspect独立validator、transport钥，域`91129655606029`。钥在
  `/var/lib/novovm/selfrun/91129655606029/B/`，0600且仅novovm可读写；包装credential
  root:root 0600，不输出、不上传、不借用旧邮箱/Relay身份。
- 实际相同User/Group和LoadCredentialEncrypted上下文通过包内AOEM KAT及两用途
  生成/解封；这不是节点systemd服务生命周期验收。
- 安装前后邮箱/Relay/Nginx/Docker全部active，PID分别944911/958/627770/1414，未变化。
- `novovm-node.service`仍not-found；没有节点启动、链库、创世、余额初始化或新TPS。

原始公开报告、安装日志、两平台release清单及实际操作脚本见
[自营交付证据包](selfrun-9a6153f1-public-evidence.zip)，SHA256
`bec384c758a5c49880fcae22b6d0becb8f8352753c96ba60d3c59b3385f86448`。
证据包仅12个明确公开文件，不含用途钥、credential、链库或签票journal。

### A/C/D现在只执行一次本机步骤

A原生UAC尝试返回“操作已被用户取消”；没有安装或生成服务钥，不绕过提权。
将上述Windows ZIP复制给C/D，先独立核对大小和归档SHA，再解到新的本地目录。
若浏览器标记下载来源，只在核对上述SHA后对该ZIP执行`Unblock-File`再解包；
不全局降低执行策略。不要复制七把资产钥或B的用途钥。

各机Codex使用管理员PowerShell运行ZIP内同一脚本；`$unpacked`为实际解压目录：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File "$unpacked\Install-Selfrun-Windows-20261106.ps1" -HostId C -PackageRoot "$unpacked\selfrun-9a6153f1-windows-x86_64"
```

C用`C`、D用`D`、A用`A`；A可直接使用原程序包根
`D:\WorksArea\SUPERVM\artifacts\product-overlay\selfrun-9a6153f1-windows-x86_64`。
脚本拒绝覆写既存release/state/服务/任务，核对源/安装后全部文件，设置ACL；
一次性Task仅在LocalService（S-1-5-19）下运行原ctl生成及inspect本机两把用途钥。
成功后注销该临时任务，节点仍只走原SCM；没有Task节点、清库、替代账本或低票旁路。
LocalService的DPAPI是该账户域保护，不宣称不同服务名之间有独立密码学隔离。

完成后只回传脚本所在目录中的`c-`/`d-`公开结果：`provision-result.json`、
`runtime.json`、`validator-inspect.json`、`transport-inspect.json`；失败保留stderr和
已有材料，不重复生成、不覆盖。不要上传`*.protected`、credential、数据库或journal。
不用预约短窗口、不重跑六组认证；公开材料齐后A沿原初始化器固定共同清单/三pin，
再安装和启动原节点，保留原4验证者/3票门槛。

以下2026-11-02内容为当时安装记录，不覆盖本节实际增量。

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
