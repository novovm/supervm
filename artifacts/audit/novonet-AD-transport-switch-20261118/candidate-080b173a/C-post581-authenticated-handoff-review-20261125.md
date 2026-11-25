# C 复核 A 交接的四机认证回执

北京时间 23:31:27–29，[A 的认证回执](A-post581-four-host-recheck-20261125.json)显示四台当前高度均为 1，A、D 在 581，B、C 在 580。原交易 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4` 在四台均为 `Received`，`indexed_height=0`。

用 C 现役 genesis 的四组验证者公钥按 `080b173a` 原公式计算验证者 ID，与 A 样本逐一匹配。激活高度为 1，排序后为 C、B、A、D。高度 1 时，第 580 轮提案者为 C，第 581 轮提案者为 B。

C 的 580 回执显示交易在本地待处理池，已有本地候选，`signed_proposal=null`。`step=Prevote`，`pending_journal`、`proposed_round`、`prevote_qc` 均为空。现役代码仅在 `Propose` 阶段调用本地提案 journal 签名路径。样本的 `successor_unavailable` 来自候选选择循环中的后继选择分支，无法据此归因于签名器。本次证据仍未覆盖 C 收到的逐票验签归集结果。

A 的 581 样本只有 1/3 nil `Prevote`、0/3 `Precommit`，两个阶段计时器均未起计。A 记录同一张原 581 nil `Prevote` 已传输到 B/C/D，接收端归集仍待证。B 当时仍处于 580。C 原 LocalService PID 24256 继续运行，B PID 571511 在线。本次复核未变更运行时或补发原交易。
