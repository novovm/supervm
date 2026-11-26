# C 原库副本：查询槽自动回收验收

回归针对 `f0507d67`，测试源码来自 C 本地 `8328aabe`。在 C 上只读取得原 RocksDB 独立副本并逐项核对 10 个文件，包含原链域标记。隔离进程作为 LocalService 以 `Existing` 打开副本，固定原 C 传输公钥，查询监听和验证者路径均为 loopback。测试只轮询冷恢复确认、回执投影、原 NativeLifecycle I/O 和原 native query，检查前后链头与共识位置相同。两次独立探针均使用已核验的 Windows Client，测试专用查询身份在同一账户托管，现役身份未更换。

北京时间 09:36，第一条认证查询得到原交易 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4` 的 `Finalized`。旧 Client 退出，达到 30.016439 秒空闲时仍持有唯一槽位。第二个查询身份完成认证，原槽由 `127.0.0.1:49556` 切至 `127.0.0.1:57383`，收到逐字段相同的 Receipt。执行成功，高度 1，索引高度 2，区块 `9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926`，原回执费字段 4000000。

Windows `rustc` 优化构建发生访问违规。测试用 `profile.release.package.novovm-node.opt-level=0` 和 `codegen-units=256` 构建，依赖保持发布配置。原生查询 4 项定向测试通过，C 现役 PID 30328、B PID 571511 在线。未重启、部署、补发或换钥。C 的结果为本机隔离验收，A 原 Client 到现役 C 的跨机复测仍待验收。

可重复用例为 `native_pipeline::service::rpc::query_slot_snapshot::original_snapshot_reclaims_idle_slot_for_another_authenticated_receipt`。测试输入为五项 `NOVOVM_QUERY_COPY_*`，只接收带域标记的独立数据库副本及本机回环查询配置。

验收日志 SHA-256 `f90224ad80a802eb0110dc095e1941afc75ae59de33592946e4e994c7b96a981`。测试 EXE SHA-256 `afda709321e55a39427f7fd1394573680784255278e1630cccac5628be52888b`。

## 原交易的独立最终性校验

北京时间 09:50，同一用例从未打开过的原库副本重新开始。旧查询空闲 30.000378 秒时仍占唯一槽位，第二个探针完成认证并得到原交易同一高度和区块的成功回执。用例从已定案归档导出证明，在本地固定的创世父状态和验证者集合上，重新计算有序回执及 UCA 授权结果的承诺、执行语句和区块哈希，对原提案和 Precommit QC 验签，再核对回执字段。验收拒绝了错误父状态、错误交易、伪改 QC 签名、回执字节、UCA 授权结果和额外尾部字节。

证明使用现有线性承诺，需要原区块的完整有序回执见证。当前支持基础转账和现有 UCA 资产回执编码。本轮验证了原库副本中的归档导出与本地验签，Client 网络拉取、新设备同步和现役部署仍待验收。没有重启现役 C，没有补发或换钥，本轮未操作 B。

本轮验收日志 SHA-256 `8f43f32350dc5ca0ddaa8c71e8f24f761381ca7e4b80fc6bfd4db4d8fd101022`。测试 EXE SHA-256 `5e73156170d679a81f025fd97655ddacf2a87f9ead4428ff0a676833ebf8bbf3`。

## Client 经认证会话独立验证原交易

北京时间 10:16，另一个未打开的原库副本以 `Existing` 和 LocalService 启动本机回环测试。独立 Windows Client 用 `novovm-native-client <客户端配置> verified-receipt <可信父状态> <交易哈希>` 取回 15,537 字节原证据，按本地固定的创世父状态和验证者集合验出同一区块、成功回执、nonce 1、费用 4000000。副本链头保持一致。可信父状态由测试从已批准的原创世材料独立准备。回执证据走原 NovoRUDP 查询流，单片上限 24 KiB，整个证据最多 32 MiB。请求绑定高度和偏移，后续片固定首片返回的整体摘要，Client 重新计算摘要并验 Precommit QC 与回执承诺。

复现用例 `detached_archive_serves_original_receipt_to_independent_client_verifier`，在前述隔离数据上设置 `NOVOVM_QUERY_COPY_CONFIG`、`NOVOVM_QUERY_COPY_REQUEST`、`NOVOVM_QUERY_COPY_FIRST_PROFILE` 和 `NOVOVM_QUERY_COPY_PROOF_CLIENT`。本次使用与前述相同的 Windows 编译设置。23 项原生查询协议测试和 5 项 Client 回归通过，原查询槽回收与篡改拒绝用例再次通过。实际原证据适合单片，多片边界及拒绝错配由协议回归覆盖。

旧资产回执采用线性承诺，完整证据包括区块内有序回执及 UCA 授权结果。节点使用 `public_asset_receipt_proofs` 配置项控制该批量披露，默认关闭。本次只在 C 隔离副本开启，未部署到现役。正式安装包固定可信锚点、发现后自动同步、跨机热切及链状态验收仍待进行。

验收日志 SHA-256 `5d3ef3123683dd7916e06dce83c16e5223b2ad3f85b54651516754b323167b55`。Client EXE SHA-256 `b6ba98e1e616173fcde6ab82599ece297f5bda6c7045e43e35397f72028d6cb0`。

## 从干净设备自动发现到独立验签

北京时间 10:33，在新的 C 原库副本上运行原 NOVONET owner。Client 从空的本地数据目录启动，生成本机通信用途身份，发行配置固定原链域、验证者权重和创世父状态。首个签名引导源不可达，第二源经认证返回 C 副本自己签名的定位记录。Client 使用同一 NOVONET carrier owner，按候选公钥建立只读 NovoRUDP 查询会话，取得原交易的 15,537 字节证明并独立验签。结果为高度 1、成功、nonce 1、费用 4000000，区块及状态根与原证据一致。副本链头保持一致，原 UDP 客户端槽没有会话。

复现用例为 `clean_device_discovers_and_verifies_original_receipt_over_same_carrier`，沿用 `NOVOVM_QUERY_COPY_CONFIG`、`NOVOVM_QUERY_COPY_REQUEST`、`NOVOVM_QUERY_COPY_FIRST_PROFILE` 和 `NOVOVM_QUERY_COPY_PROOF_CLIENT`。Client 入口为 `novovm-native-client <发行发现配置> discover-verified-receipt <交易哈希>`。现有 NOVONET owner 同一轮发布本机定位记录和开放查询，避免首次认证读到空候选。节点入口类型检查和三项原载体回归通过。测试使用回环网络和已固定的创世锚点，跨机切网、连续状态同步及共识追赶仍待验收。现役 C 未部署或重启，未接触 B。

验收日志 SHA-256 `b2763ceaa8f02702a31fbd2b614c81e356c91d41bb706c2b58626a88449b6919`。Client EXE SHA-256 `2887548f508172b2ce2ae961880d1cf9ff707d95d18d8768a755d2743a4f8d73`。

## 逐块验证区块头与认证状态根

北京时间 10:46，`novovm-native-client <发行发现配置> sync-chain` 经自动发现的只读 NOVONET 会话从创世父状态同步至副本实际高度 2。Client 独立验证高度 1、2 的 Precommit QC 和父状态衔接，将通过的证据按高度持久保存。高度 2 的区块是 `7f2d438862e6bd2934862e4cafcb86dc3bf6ffa0a414d532014e420efa1aafc6`，认证状态根是 `418cb1c74c5121d43d4a392a309a052d0f6a8a513bc9377b6aa503d80591e439`，状态版本为 2。重开时从发行锚点重新验过本地证据，恢复到同一高度、区块和状态根。修改本地证据后，Client 在重新连接前拒绝恢复。

以上步骤追加在 `clean_device_discovers_and_verifies_original_receipt_over_same_carrier` 用例。区块头同步阶段关闭了整批资产回执导出。原交易独立验签、23 项查询协议测试、5 项 Client 回归及原生载体对象交付回归通过。Windows 编译器优化阶段出现访问违规，本次隔离验收对 `novovm-node`、`novovm-network`、`novovm-relay`、`novovm-adapter-evm-core` 使用了 `opt-level=0`、`codegen-units=256`。完整状态数据重放、持续同步和跨机网络切换待验收。现役 C 与 B 均未操作。

验收日志 SHA-256 `4e4a04858dac142f9529c42a4b0ee699bf92709c0c1d0d522bb6f9c7461abdb8`。Client EXE SHA-256 `6fc10317aa5748e6a48cd816284c44abeb68ca21ffbf0ed2642a306a8838a8fa`。

## 下载并逐节点验证原生状态树

北京时间 11:02，在 C 的新隔离副本 `state-sync-21b46859f4d4` 上，以 `LocalService` 运行同一验收用例。打开前对照了 10 个 RocksDB 文件的 SHA-256。Windows Client 通过 `novovm-native-client <发行发现配置> sync-state` 在已验证的高度 2 上下载 97 个 Patricia 树节点，验证全部节点哈希、分支路径和 49 个叶子。状态根为 `418cb1c74c5121d43d4a392a309a052d0f6a8a513bc9377b6aa503d80591e439`，状态版本为 2。重开重新校验同一状态树，下载数为 0。测试同时重新验证高度 1 原交易的 Receipt 和最终性证明。

状态导出开关默认关闭，本次只在隔离进程中开启。关闭时状态请求被拒绝。开启后，服务端只向同一原生查询路径提供能证明属于已发布状态树的节点，测试中无法证明可达的哈希被拒绝。修改本地根节点文件后，Client 拒绝恢复已验证状态。副本链头保持在原高度和原区块。验收前后现役 C 均为运行中的 PID `30328`，本轮操作限于 C 的隔离副本。

同一用例完成用时 82.42 秒。查询协议测试 24 项、状态树回归 47 项、Client 回归 5 项通过。Windows 隔离构建对上述四个包继续采用 `opt-level=0`，`novovm-node` 的 `codegen-units` 改用 128 后通过构建，其余三个包用 256。状态下载是有界的手动 Client 入口。大状态分批同步、持续追赶、跨网热插拔和基于固定程序的首次同步证明待后续验收。

验收日志 SHA-256 `9840bdd1e27ffb24ea5fa24ca9d39e87daf77ed2041be4ce498b39145b11ea16`。Client EXE SHA-256 `ecbcf9bfac6d732f16c1817b28fba10ab9d6bd8e11eca0df14fbb6f5b2616081`。
