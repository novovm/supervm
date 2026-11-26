# A 的 Windows x86_64 基础候选

取远端 main 的 `de16d52553c1ac834eedcdf841d10caf37ed5894` 为构建基线，保留已核验的 `a53a7a74` 实包。

- `807cc81f` 修改生产 Node 的 Iroh 原生查询服务。正确链域且完成原认证的同一传输身份可以在另一条连接恢复唯一查询槽，旧连接的代次失效，其他身份仍受容量限制。这项运行时变更要求更新候选。
- `b1e26a1b` 增加 AOEM 跨纪元、验证者集合及权重的离线 QC 回归和规格。其测试与文档不改变生产行为。 `de16d525` 补录 D 的证据缺项，亦无生产运行时变化。

按正式 `scripts/novovm-package-product-linux.ps1` 以 Rust/Cargo 1.94.0 和默认优化配置构建，目标 `x86_64-pc-windows-msvc`，复用原 Cargo 缓存。未修改 Cargo.lock 或脚本。Visual Studio 18、VC 14.50.35717、Windows SDK 10.0.26100.0。北京时间 2026-11-26 13:19:38 正式脚本退出 0。

| 核验对象 | SHA-256 |
| --- | --- |
| Cargo.lock | `299030421087c2ee5b16330a436f16baf7c7c3a06f9cae23ab50af10aba4c46f` |
| 既有批准的 AOEM 来源包 | `49657f70273ee3070d8a439e172bb75d00e80dcf44e911e7d02b569dee337c72` |
| AOEM FULLMAX Core | `f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa` |
| 新 Windows ZIP（164,244,268 字节） | `ac056f3aadb277076c0ce3300099b74d75c9f4d80d55cd0e3dc99063dc289704` |
| release-manifest.json | `83e784e9bd34f3cc4fc4b79ac2915b0b5c722076e2ad4c2a7ec2e206702dcd8d` |
| CHECKSUMS.sha256 | `b9fee7b3691bea6d41d5cde0682905b4a9824992365bb3a0bebd03f4926620b7` |
| AOEM 资格报告 | `ce048240732a1f8e55450db64efc4b842d8edb1e51a8e88e87588580537d93b3` |

10 个 PE32+ x86_64 正式二进制均通过清单与实际字节核对，ZIP 回读复核 96 个文件。AOEM ML-DSA-65 标量、统一语义和混合批量验签 KAT 通过。Windows SCM `existing` 服务入口合同检查通过。

| 二进制 | Cargo 功能 | SHA-256 |
| --- | --- | --- |
| novovm-node.exe | default, iroh-transport | 69b644ef4b30242be8a2c5458ca877a09a2eaf89b86cecfb1d3a8c3339d5caa4 |
| novovm-native-client.exe | default, iroh-transport | b552e3710f23a56d9e5a883fb18afec3061cad027a1730f1c4b5c88cf3d62878 |
| novovm-product-relay.exe | default | 0555e96577c59dcd4d420976bea07ec26457704c7c21a868b729479382e7f401 |
| novovm-product-node-overlay.exe | default | 1142d53b2021a831f2c0ac83636f0f5295c485b255beeddb592e9a1923acfd43 |
| novovm-product-nat.exe | default | 822578f46ab73c89187b0b57a31e50ee0ea0dc8cf149c7e92f555a003a1b0b94 |
| novovm-product-peer.exe | default | 103225ad170e0f6972b65d44077286ead0ec8cb4fe9589fe4ab4aef63f57992f |
| novovm-product-evidence.exe | default | 0ee2f0988d251362b7af92f3ba34902f115647d574b64c0a1163de7588be2aea |
| novovm-rendezvous-service.exe | default, iroh-transport, tls-transport | 1c29809babd8fb221c401c2a8944d327adab2aa7a05c39ee4f1692fe2ad7836a |
| supervm-relay.exe | default, iroh-transport, tls-transport | f25682da1d083bc80267404f18d7879d29cc83c6378bdc0a7fc746a495a40025 |
| novovmctl.exe | iroh-transport | 1672b55dc497c100e6a04f25ed95446d19eb4e4a04c05486530388d8695610ff |

A 在原库独立副本上另跑正式源码的回环接管用例。保留旧流时，同一传输身份另建连接，在 88 ms 内重新获得并独立验真同一原交易 Receipt。旧流不能再取回执，其他身份和错误链域均被拒。独立 AOEM 跨纪元、集合和权重的 QC 用例也通过。该结果来自隔离测试构建。现役节点、原库和原钥均保持原状，未补发交易。

两入口发行配置仍待批准，基础候选未内置该配置。包和二进制走既有受限取件渠道，公开仓库只保存上述脱敏结果。
