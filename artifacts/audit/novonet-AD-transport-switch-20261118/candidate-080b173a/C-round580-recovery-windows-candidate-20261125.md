# C 第 580 轮 Windows 恢复候选包离线核验

2026-11-25 北京时间，C 使用统一产品打包器构建 `x86_64-pc-windows-msvc` 发布包，源码固定为 `9f223abf1623efaf196188cef5cacc0a680837e8`。相对于 A 已用于现场恢复的 `8cf76761552a55ab85a85660ae8475d8c7e1ded4e`，Cargo 清单、产品源码、脚本及 vendored 依赖一致。

`novovm-common-9f223abf-C-recovery-windows-x86_64.zip` 为 157,962,658 字节。逐项复算 95 个 ZIP 成员，并与发布清单的 92 项和 AOEM 的 40 项清单核对。AOEM 资格报告为 `verification_qualified=true`，实际 DLL 哈希与现役固定值一致。

| 文件 | SHA-256 |
| --- | --- |
| ZIP | `bc79f823af6548aa4a03d766659b29a5c1bc789d28798eb6178c20895234f6a2` |
| `release-manifest.json` | `e564b966d641880d0e7a1a1f96184a827079000642fd755a2c099562af06e56a` |
| `novovm-node.exe` | `5b2831c994eda2c2da270a75c9174564346a3cd1ebd371e7b36f93969e7b6d53` |
| `novovm-native-client.exe` | `accd675963312410bce738a9f057853e781eb88a541bebc0f911eac2976134f6` |
| `novovmctl.exe` | `936029864b09a4aedfaf7e211205d93036b4e79854f17cb963dda3301c1babb7` |
| `aoem_ffi.dll` | `f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa` |

包内 Node 返回 `windows-scm-existing-v1`。包内 `novovmctl lifecycle --action start` 对隔离清单生成 `applied=false` 的计划，沿用 C 的 `LocalService`、原库 `existing` 与原验证者、传输身份引用。原 A+B+D nil Precommit 证书重新核对为 `f5614fada23e812106dde96b770f2f120f2f68cdac62bc2e58e97bd03200f363`，本次离线计划未将其加载到现役运行时。C 原 PID `24256` 继续运行，B 原 PID `571511` 在线，原交易未补发。
