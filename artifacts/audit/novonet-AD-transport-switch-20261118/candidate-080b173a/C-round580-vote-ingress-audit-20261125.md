# C 第 580 轮原签票接收核对

北京时间 11:12–11:13，C 的原 `NOVOVM-Native-Validator-C` 服务仍以 LocalService、PID 24256 运行，UDP 41052/42052 仍归该 PID。实际 EXE SHA-256 为 `0a5fe94e6d191567f570e6e6754583110ff37804668eadcc9ce5c11eda83c0d3`，对应已部署的 080b173a。B 通过既有 SSH 只读复核仍有同一现役 novovm-node 进程，PID 571511。未重启、变更配置、发票或补发交易。

## 已经能够配对的观察

A 的最后一组 1/580 样本在 10:55。其恢复发送的是原签名的独立 Vote 消息。A 侧对下列两张票都记录了送达 C 的原生传输完成。样本中 C 路由为已认证直连 UDP 192.168.71.73:41052。

| A 的原票 | 片段 SHA-256 | 发往 C 的完成记录 |
| --- | --- | --- |
| nil Prevote | `deae63af1c0057f4913d95f5b49dfd2cab5475e9a7e07c3c8e6cf3f64205b38f` | 6 次，最后序号 1566 |
| nil Precommit | `ba1fbb81903ee6c5581c9c43f1cc63b6287d536c69bc26a9aa8f304d36bdc4b6` | 6 次，最后序号 1567 |

A 本机 collector 在同一轮验签归集的 nil Prevote 为 A+C（2/3，B、D 未见），nil Precommit 为 A+B（2/3，C、D 未见），两个计时器均未起计。这证明 C 的原 nil Prevote 已进入 A 的 collector。B 的 Precommit 出现在 A 的 collector 也不能替代 B 的 Prevote。A 最后对 C 的认证结果仍是 Prevote，原交易 Received，匹配提案、prevote QC 和待确认 journal 为空。[A 侧原始记录](A-round580-recovery-live-20261125.json)

## C 接收端的证据边界

080b173a 对独立 Vote 走原验签及当前上下文的 collector 插入路径。同轮 FarRoundWitness 走另一条路径。A 本次发送的是独立 Vote，现有证据不能把它归因为 C 的旧 FarRoundWitness 门禁。仍需按片段匹配 C 的消息验签结果和 collector 写入结果（新增、同票重复、拒绝或丢弃）。

C 现役未包含后续 89c64f82 的 collector/pacemaker 诊断字段，服务现有文件未发现本轮进度或逐票输出。活跃 locator 文件只记录发现状态。现有认证查询只返回共识步骤及 QC 等摘要。因此两张 A 原票均已确认到 C 的原生传输层，但 C 是否验签、是否计入权重未观测到，不能确定 C 所缺第三份权重是哪位签者。A 侧可独立确认的缺口是：Prevote 还差 B 或 D，Precommit 还差 C 或 D。

下一份可定案的证据是 B、D 各自 1/580 的原 Prevote 是否存在及是否送达，和 C 侧按签者、阶段和值的验签归集状态。当前 C 未部署诊断版，原服务、数据库和原交易继续保持。
