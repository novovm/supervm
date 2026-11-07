# READY_NOVONET_CE1_C

2026-11-08，validator-C 同步 main `46fbeac0`，按原共享传输交接第11节完成一次候选接收。

- ZIP 实测150,354,733字节，SHA256 `79cdfcf8636ddb881b2c9fa56d28c990ef5b12c8c7010f52cff0fea3a2576c9b`，与A独立pin一致。
- 程序源码固定 `ce1f54ea545e69dc64763f1f24f6048d90e4ef12`，manifest SHA256 `6f9f222f738ca990636572ffcad6256b1cd294b082254912bfbd604f7081978f`。
- 解压到新目录 `target/novonet-ce1-C-incoming-20261108`。归档路径、重复、链接及额外文件检查通过，77项制品逐文件长度/哈希通过，根checksum 77项及SDK辅助checksum 26项通过。
- 包内接收脚本SHA256 `c4ecbf44ad620f7a9caa605b4770feaabdf9dbdb272243660adf5fc2a237a481`，与Git及交接pin一致。
- 实际执行包内脚本，参数 `--host-id C`，结果目录 `ready-ce1-C`。退出码0，返回 `READY_NOVONET_CE1_C`，复核77项共411,022,253字节，实际node包含 `iroh-transport` feature。
- 包内AOEM core SHA256 `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。实际加载路径位于本次解压目录，scalar/unified/mixed-batch验签KAT及sign接口形状检查通过。
- 实际node离线合同为 `windows-scm-existing-v1`、`native_execution_tick`、`existing`，副作用字段均为false，两份stderr为空。
- 当前LAN地址为 `192.168.71.125/24`，接口24，链路1 Gbps。

原报告时间为UTC `2026-11-07T16:24:46.805830+00:00`，对应北京时间11月8日00:24。
原始五份输出见本目录 `report.json`、`runtime.stdout/stderr` 和 `node-contract.stdout/stderr`。
其原始字节由 `.gitattributes` 保留，长度和SHA256登记在 `public-receipt.json`。
独立归档核验登记在 `archive-verification.json`。

本轮范围为包接收与离线资格。单中继、双中继及恢复的实网结果等待A组织后续验收。
已有安装、用途钥和节点状态沿用原现场，接收脚本副作用字段与此范围一致。
Git回传供A汇总，A签收状态需由其确认。
