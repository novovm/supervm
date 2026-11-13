# Phase 4 复核资料（不是节点启动指令）

本轮已执行完毕，不自动重复采集。所有命令以 SUPERVM 根目录为工作目录。
私有工作目录为 `target/c-boundary-phase4-20261114`；原始转储和旧证据保持原位。

1. 用 `git archive a25452f4c49be37cd984e650f2079642e6c273b3` 导出独立源码，
   包含根 Cargo/工具链、crates、vendor、aoem、config/configs、scripts、docs；不切换 main。
   对照 source-verification.json 核验固定文件；不创建开发分支。
2. 节点完整构建命令见 build2-command.json。设置单独 CARGO_TARGET_DIR 到上述目录的 build，
   设置 CARGO_BUILD_JOBS=4；node 包库/bin 均 debug=2，原 opt3/overflow 与 iroh 特性保留。
   本轮只编译节点，没有执行生成的 novovm-node.exe。
3. 隔离 harness 的 main.rs、Cargo.toml、Cargo.lock 在 harness 子目录。
   私有工作区实际结构为 snapshot-harness/src/main.rs 及原源码逐字复制的 profile.rs、
   profile/traffic.rs；它们不重复提交，来自第1步的固定源码。manifest 的 ../source 指向导出目录。
4. 隔离构建：`cargo +1.94.0 rustc --locked --offline --release --target x86_64-pc-windows-msvc --manifest-path <私有目录>/snapshot-harness/Cargo.toml -- -C debuginfo=2 -C strip=none`。
   本轮 jobs=2，使用相同独立 build 目录，未安装程序。
5. map-functions.py 使用 pefile/capstone 和既有 Phase3 reference JSON，按归一化函数代码
   匹配实际 PE 函数及 unwind codes，不沿用原节点绝对地址。node/harness 映射均已保留。
   Python模块路径与CDB路径沿用本机已有工具；迁移路径时应审阅，而非下载替代包或强制符号。
6. collect-boundaries.py 只启动 c-profile-boundary.exe，最长120秒，不启动节点、不连接网络。
   `PHASE4_PREPARE_ONLY=1` 只生成命令。本轮最终命令为 capture3.cdb；此公开副本把路径
   替为 <SUPERVM>，不可原样作为 shell 路径执行。实际运行的未脱敏版本只在私有目录。
   begin/end public symbols 被链接器折叠到同一地址，使用一个切换断点。
   以 xb 打开原日志，已有日志时拒绝覆盖；后续不得为了重跑删除它。
7. check-boundaries.py 仅解析现有 capture3-private.log，逐线程/调用序号核对两条正常分支，
   并单独核对实际目标退出码、调试器退出码及解析错误。可只运行它重新审阅已有记录，
   不需要重新运行样例。

公开 capture*.log 是隔离程序原始输出的工作区路径脱敏副本；private-evidence-hashes.json
给出本机未脱敏原件 SHA256。capture-records-public.json 数值为十进制，日志地址为十六进制。
首次实际失败指令仍未知，公开结果不能用来关闭 C 崩溃问题。
