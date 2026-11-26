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

## Windows Client 自动同步和路由恢复

北京时间 11:13，C 的新隔离副本 `follow-state-23a85d888879` 在 `LocalService` 下让同一个全新 Windows Client 进程执行 `follow-state`。错误的发行配置哈希在创建本地身份前被拒绝。配置正确时，Client 从空数据目录本地持钥，经已固定对端公钥认证的原生 NovoRUDP 下载并验证高度 2 的 97 个状态节点和 49 个叶子。

隔离回环转发器停止转发原生 UDP，两个引导文件一度均不可用。Client 保持进程并进入重试。测试恢复第二份签名引导文件，第一份持续缺失。同一 Client 通过第二份来源找到原服务公钥，经认证 Iroh 备用恢复。恢复原生报文后，Client 重新认证并切回原生 NovoRUDP。三段的对端公钥、设备身份、已验证高度、状态根和状态数据一致，累计下载数始终为 97。测试总用时 40.87 秒。现役 C 前后均为运行中的 PID `30328`，操作限于 C 的隔离副本。

复现沿用同一 `clean_device_discovers_and_verifies_original_receipt_over_same_carrier` 用例，设置 `NOVOVM_QUERY_COPY_FOLLOW=1`。不设置此开关时，独立 Receipt、区块头、状态树和篡改拒绝的原回归用例仍通过，耗时 82.24 秒。Windows 装包测试以隔离装配输入核验了安装脚本入包、精确的带哈希启动命令和错误配置哈希拒绝。现场清洁设备登录自启、实网换接口、动态获取新版原生地址和大状态同步仍待验收。

自动恢复日志 SHA-256 `b9d24d3a43aca212d58e1b6d53b83c2cd1f0dabe69aa983ece06849bb77b0ac9`。事件 SHA-256 `166763039c664347175687c8b59891ae79dbf5af163bb88c0e03e764a4f7b212`。原用例回归日志 SHA-256 `87ef2f729164072ac47df0c53750f3c0e502cb13c1fa7d7749be37ce4e697898`。Client EXE SHA-256 `dcb03e00c6267b3d3cec8bd5c7da6654b731057d4ad5edd8851693ea71894103`。

## 法定权重恢复、原库追赶与少数派停止提交

北京时间 11:20，两组测试分别以公钥排序最小和最大的节点作为晚回归者。每组在 C 本机隔离目录运行四个新构建的原生节点进程，通过原 NovoRUDP 和真实 AOEM 执行。四个等权验证者的法定门槛为 3。在线的三个节点先一致推进到高度 40，晚回归者随后从自己的原库恢复，并在追赶中被中断后重新启动。两组均保留原库、配置、钥和原签名出站记录，测试交易补发计数为 0。

仅两份权重在线时，每组连续 12,024 毫秒没有提交新区块。恢复第三份后，原待处理交易终结。第四份恢复后，每组四个节点均到高度 46，并就同一状态根及归档回执达成一致。真实 AOEM 的计时器、journal 确认和锁定追赶本机回归另行通过。验收范围为单机四进程等权集合。现役 C 复核为运行中的 PID `30328`。

使用 `NOVOVM_RESIDENT_NODE_BINARY` 指定同版本的 Windows `novovm-node.exe`，用 `NOVOVM_AOEM_TEST_LIBRARY` 指定已核验的 AOEM DLL，依次运行忽略标记下的 `native_only_recovery_late_lowest_validator_original_database_and_minority` 和 `native_only_recovery_late_highest_validator_original_database_and_minority`。原生节点 EXE SHA-256 `9836ef9823294b2edbdef32db50c8abbc376a82066b05a3f244ceb8109b8ea3c`。两份复现报告 SHA-256 分别为 `1ec9df851dc40414a85d841b03fb8cf876bbc47ec09af1119ec0048ff25077a7` 和 `57592a2daaded9e46a29a1f9d13836d71766b46ae0e22715ed7879d0a92221ec`。

## 签名原生查询地址变更后的自动恢复

北京时间 11:34，在 C 的 `signed-address-27f1fb69696e` 新隔离副本启用 `publish_native_query_address`。空目录 Windows Client 经发行配置预置的回环转发端口 `49560` 达到高度 2。关闭该端口转发后，Client 进入重试。当第二份来源可用时，Client 通过原目录取得链域和服务公钥绑定的 `NVLOCR03` 定位，使用序号 1 上签名的真实节点 UDP 端口 `45192` 完成原生认证。旧端口在整个恢复期间保持停转。两次结果中 Client PID 均为 `39232`，设备身份、状态根与 49 个叶子一致，下载累计仍是 97 个节点。

Client 将定位记录保存到原验证检查点。修改缓存中的端口后重启 Client，签名验证失败，拒绝使用改过的地址。回归 `signed-address-legacy-6b5b0802f879` 仍通过原生、Iroh 备用、原生重新接管。18 项目录回归通过，覆盖兼容原定位版本、地址字节篡改、版本替换、错误链域以及到期拒绝。新地址发布默认关闭。此阶段在绑定具体地址的隔离回环环境验收，实机换网、通配监听的接口发现与公网 NAT 地址验证待后续验证。

复现使用同一隔离用例并设置 `NOVOVM_QUERY_COPY_FOLLOW=2`，原路径回归用 `1`。切换日志 SHA-256 `207b93be5a32e3a7b06eaa2b640d42d6065120209ec36e3f151e296c2916a37e`，事件 SHA-256 `1f0e8f9840315faf8da8e4ac9c292684ec414deb953895731ff582d81f1f66c3`，旧版路径回归日志 SHA-256 `1fd8e54bf084079b4603d9412baa79e2426d7d2e63df2b985dd961918a2f6737`。Client EXE SHA-256 `c9d57a36c9863629d725ee0a4aa83ceecf8877d51fcc7cdea338fb1c9bc85be0`。

另以 `signed-address-receipt-36113dfaac73` 重跑原交易 Receipt、区块头和状态树校验。原交易仍在高度 1 独立验证取得成功及最终性，高度 2 的状态树和篡改拒绝回归通过。日志 SHA-256 `b5a907a6073a03232e571fc01eb8bbd552c021bd0c9ef285dd70b3b2763ff28f`。三次隔离操作前后现役 C 均为运行中的 PID `30328`。

## 首次同步的发行检查点和待验证执行承诺

北京时间 11:50，在 C 的 `checkpoint-policy-0b9b9cd52cc3` 新隔离副本设置 `NOVOVM_QUERY_COPY_CHECKPOINT_POLICY=1`，使用同一原归档用例。Client 从已有独立信任起点验签并达到发行配置固定的高度 2 和区块哈希，验证对应原状态。错误的高度 2 区块哈希在空目录下载和重开本地缓存时均被拒绝。正确检查点的发行有效期过期时，缓存中的状态也不能报告就绪。将最低高度设为 3 时，持续 Client 只报告 `verified_height=2` 和 `required_height=3` 的等待事件。有效期按发行配置固定的 UTC 截止时间检查，签名区块的确定性执行时间仍进入原承诺。

已完成 5 项共识声明和 16 项业务关系回归。新测试以已验证的原生共识声明重建直付 `NVEXEC01` 的 152 字节输出，与原共享 guest 关系的结果逐字节一致。换执行合约或状态根的反例拒绝。原 C 归档 UCA 区块经真实 QC 验证，但不符合直付 guest 的业务关系。该测试覆盖独立预期输出绑定，完整 UCA 的真实 ZK 生成和统一 AOEM 验真仍待接通。

检查点验收用时 26.50 秒。现役 C 前后均为运行中的 PID `30328`。Windows Client SHA-256 `9cecf085630e619d8af23f50a89932163bba27c88d3b0c8f5b6f550568f39c37`，本轮隔离日志 SHA-256 `8f14276d01f74995934a5f74656df0adf0efda70cbc4c9d91f5f54ed745d48ad`。Windows 测试编译器在 128 个代码生成单元时出现访问违规，隔离测试用 64 个单元重构通过。Client 使用 128 个单元成功构建。

另外在 `checkpoint-policy-legacy-2575868d00a8` 以原无检查点配置回归自动恢复，42.21 秒通过，仍完整经历原生、Iroh 备用和原生重新接管，并复用原状态缓存。日志 SHA-256 `4e310090504603b87e343b34831f126c426c657b4f90bae2a54c13d0148fdfa6`。

## 原 UCA 资产区块的离线执行核验

北京时间 12:06，C 的 `uca-replay-2290f435b5ce` 隔离副本以 LocalService 按 10 个已核对 SHA-256 的 RocksDB 文件打开。验收以原签名交易和原父状态的读取见证构建高度 1、2 的隔离归档，分别为 34,237 和 34,571 字节。Windows Client 独立验原 Precommit QC 和资产交易签名，检查见证属于已经认证的父状态，用现有 AOEM 执行后核对状态根、回执批次承诺和完整执行声明。两块均与归档一致，高度 1 含原交易 `68d83d0b…9bd6cd4`。

空 Client 目录可直接从发行创世锚点核验高度 1，且不会写入本地状态或生成通信钥。高度 2 在缺少已认证的高度 1 父状态时被拒绝，经原 `sync-chain` 验证区块头后通过。复测时由测试进程持有检查点写锁，离线 Client 仍能读取并复核。分别篡改 QC 签名、父状态节点和资产签名，两块均在对应环节拒绝。另用错误发行配置 SHA-256 验证在读取归档前拒绝。

复现沿用 `clean_device_discovers_and_verifies_original_receipt_over_same_carrier`，设置 `NOVOVM_QUERY_COPY_UCA_REPLAY=1`。Client 入口为 `novovm-native-client <发行发现配置> verify-uca-execution <配置SHA-256> <归档文件>`。真实副本用例用时 14.51 秒，另有 27 项相关回归通过。高度 1 和 2 的回执批次承诺分别为 `ecf5d727891d35eecb91539ed2d49f2ba762a57780918198b47a55e0e3f70c11` 和 `8dc7832a8074bfa97ce8a8bba7646e862fe9183e2138035de104bb77fcd72abf`。本阶段验收离线重算，UCA 零知识证明和节点间自动分发待接通。原始归档留在隔离目录。现役 C 前后均为运行中的 PID `30328`，未操作 B。

Windows Client SHA-256 `0b440fd21d543b45121f323081457ea8b534c1960976efcd631ae49dce0d512e`。隔离日志 SHA-256 `7947612169445137dd026eb39bf5a90d53a5c22da84d8d7a0620a728494a5baf`。

## Windows 安装、免注册同步与冷恢复

北京时间 12:35，C 的 `windows-install-94c6c01d58e7` 副本以 LocalService 启动独立回环服务。Windows 当前会话的非管理员进程只传入验收清单 SHA-256，运行原安装脚本，校验并将 Client、AOEM 和签名入网文件复制到该用户的 LocalAppData。系统计划任务限定当前交互用户和普通权限。新目录首次启动时由 Client 在本机持钥，经有效签名来源认证后获取高度 1、2 区块头与高度 2 的 97 个状态节点。已认证状态根为 `418cb1c74c5121d43d4a392a309a052d0f6a8a513bc9377b6aa503d80591e439`，安装的 Client 在运行中通过独立 UCA 重放再次核验区块承诺。

重复运行安装器保留原 Client 进程与通信钥。团队移开验收解包目录和一份已验证状态节点，再触发同一 Windows 启动任务。新进程沿用同一 DPAPI 保护钥，重新认证连通，下载并验回同哈希的缺失节点。第一次 PID `39888`，第二次 PID `42788`。隔离日志记录了旧 Iroh 会话占槽和超时，Client 随后自动恢复。验收结束已注销三项临时任务，现役 C 前后均为 PID `30328`，未操作 B。

复现使用 `NOVOVM_QUERY_COPY_WINDOWS_INSTALL=1`、原隔离测试和 `scripts/tests/windows-client-install.tests.ps1`。官方打包脚本的 Windows/Linux 装配回归通过，包含发行配置 SHA-256、两份引导文件及篡改拒绝。实际安装用例运行真实原打包产出的 Client、AOEM 和安装脚本逐文件核对，使用 C 隔离服务签名的入网资料及限于验收的清单，范围为同机 Windows 任务与回环网络。副本用例历时 40.15 秒。Windows 优化编译的访问违规阻断了本轮完整带发行入网资料的包生成。隔离验收覆盖当前交互用户与同一启动任务的重触发，跨设备、实网换路、系统重新登录和正式发行包还有独立关卡。

Windows Client SHA-256 `0b440fd21d543b45121f323081457ea8b534c1960976efcd631ae49dce0d512e`。隔离清单 SHA-256 `ff1614b9b19300dde85af7b0cdc31cf68d0be42b09b7cde7b88303378c8cfee7`，隔离日志 SHA-256 `9a6c5e984ee4a26396c202cd6ab4a89c39b44d8ed38a6c9806de397ea3bb9a26`。

## 安装引导地址失效后的冷入网

C 的 `bootstrap-contact-f4de776fad5c` 原库副本在 LocalService 上通过回环测试。干净 Windows Client 只获得两组固定联系地址及传输公钥，首选源无法连通。它经第二源完成原 NOVONET 认证，取得当前签名定位，独立验回原交易的 15,537 字节 Receipt 证明。区块头同步到高度 2，随后下载并验证 97 个状态节点和 49 个叶子，重新打开时下载数为 0。篡改本地头文件和状态节点、不属于认证状态树的哈希及关闭披露时的请求均被拒绝。现役 C 前后均在 PID `30328` 运行，未操作 B。

独立的真实 Client 与 AOEM 测试覆盖两份过期文件上的旧拨号地址都不可达。仅含旧文件的配置无法找到源，带固定联系地址的配置经第二个对端取回新签名定位。移除两份文件并使用新的本地数据目录后仍通过。原文件发行配置和重启持钥回归也通过。Windows 包装配模拟覆盖纯联系地址配置，当前公网独立入口和跨机换网继续单独验收。

复现将 `NOVOVM_QUERY_COPY_BOOTSTRAP_CONTACTS=1` 用于原库副本用例。独立测试使用 `clean_device_recovers_expired_bootstrap_files_through_pinned_contacts`，以 `NOVOVM_DISCOVERY_AOEM`、`NOVOVM_DISCOVERY_AOEM_SHA256` 和 `NOVOVM_DISCOVERY_CLIENT` 固定真实构建。Client SHA-256 `153bd1466f9064cfdf3ebe16a7f722fb4e98d53c10d5332c99a9709667cdc3b6`。原库日志 SHA-256 `390db1775a277df40165cec40ae679877c99dd6a2c3a7933cdb5e76b67a8e02d`，两项 Client 测试日志 SHA-256 `1d122e89681e67fddd4e09edcb999a4ed23f3fff1e1978ea140bd37b8190e739`。

## Iroh 备用查询槽同身份接管

北京时间 13:04，以 LocalService 打开 C 的原库独立副本 `iroh-slot-339e841e8f6a`，复制前后核对 10 个 RocksDB 文件。测试保持旧 Iroh 查询会话打开。另一个认证身份通过选择链查询服务后被占槽检查拒绝。同一身份声明错误链域也被拒绝。两项反例之后，旧会话仍能取得并独立核验原交易 `68d83d0b…9bd6cd4` 的高度 1 最终性回执。

相同传输身份随后通过另一个独立绑定的 Iroh 套接字重新认证并选择正确链域。从重连开始到再次取回并独立验真同一原 Receipt 用时 80 毫秒。旧会话随后无法再取回回执，结束旧会话后新会话仍取得同一份证明。RPC 仍使用一项查询队列，其他身份继续等待占用结束。复现参数为 `NOVOVM_QUERY_COPY_CARRIER_TAKEOVER=1`，复用 `clean_device_discovers_and_verifies_original_receipt_over_same_carrier`。测试在同机回环运行。现役 C 前后均为 PID `30328`，未操作 B。本记录为代码与隔离验收，现役部署及真实换网分别验收。

测试 EXE SHA-256 `f3cba041ac5156fc9e525fb6733fd307ebe4f5d9fa65009bf139947c30c75cf2`。隔离日志 SHA-256 `745fa9894647bdc82358de4031e8d3d3ce75575bd3a627e09bcbe8a8ea149d84`。
