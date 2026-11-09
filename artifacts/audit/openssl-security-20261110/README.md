# OpenSSL安全依赖修正与原网络观测候选

输入源码：`55f947c9344bcdff0d7c17288439acd35df4ff38`。
原自动CI [42683324372](https://github.com/novovm/supervm/actions/runs/42683324372)
的Rust security job `129383748040`在根Cargo audit失败；其他作业当时仍运行。
不是SQLite回退，也不是EVM、ML-DSA或NovoRUDP测试失败。

## 原因与最小修改

原锁文件含`openssl 0.10.76`，RustSec `RUSTSEC-2026-0463`指出安全函数
`aes::unwrap_key`的输出长度断言反向，可能允许底层C函数写出缓冲区；修补版本
为`>=0.10.78`。本地已读原公告及CI原日志，不忽略该公告、不改安全门禁。

实际Linux依赖链为`lettre 0.11.22 -> native-tls 0.2.18 -> openssl`，
由原node及rollout-policy邮件消费者引入。仓库crates未找到直接调用unwrap_key，
但这不等于已证明所有传递调用不可达，也不作为忽略公告的依据。

用Cargo更新根锁文件，仅两包发生变化：

- `openssl 0.10.76 -> 0.10.78`；
- `openssl-sys 0.9.112 -> 0.9.118`。

runtime锁文件没有openssl/native-tls，不做无关更新。没有修改AOEM、SQLite、
RocksDB、身份、网络协议、资产规则、资源限制或超时。新程序继承第30节的
有界失败阶段观察；依赖修正不能解释现场尚未归因的发现失败。

## 本地结果

根/runtime Cargo audit均退出0，均为0个vulnerability；既有informational/yanked
警告保留，不表述为零警告。根/runtime Cargo deny均退出0；未新增ignore或修改策略。
Linux实际node及rollout-policy消费者严格Clippy通过。
Windows/Linux release原network owner各8/8（234.34s/229.99s）、discovery各2/2
及node严格Clippy通过。两平台rollout-policy现有测试各1/1通过，bin/doc的零项
不另算通过用例；该用例验证原HTTP错误回执，不冒充真实SMTP TLS握手验收。

原始记录在`target/openssl-security-20261110/`及
`target/novonet-failure-observation-20261110/*-openssl-final-*.log`。
已归档[原始输出与复现入口](original-evidence.zip)，全成员大小及SHA256独立核验；
归档和锁文件哈希见[evidence-manifest.json](evidence-manifest.json)。
制品构建继续复用原打包器、包内资格检查及固定AOEM，而非复用旧node二进制。
本记录不代表新候选包、云端完整CI或A/B部署通过。

## 现场与下一步

A/B仍为3fa程序；已取得的真实A→B原生Object DONE证据继续有效，见共享交接第30节。
完成本地受影响验证后固定新源码，制作两平台包，检查实际包内程序/库，再按原工具
更新B、核验后更新A。原库existing最快且能检验恢复，本轮无须清库或换钥。
用户开发期停服授权已明确，不重复申请；C/D本轮不操作。
新观察部署后依据实际失败stage定位，不把累计失败数或空闲bound routes=0当作根因。

这是内部开发推进，不是公开发布、四机中继/多跳、资产业务或性能签收。
