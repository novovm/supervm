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
