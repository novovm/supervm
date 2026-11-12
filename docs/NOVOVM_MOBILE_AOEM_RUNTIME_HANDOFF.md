# Android / iPhone AOEM 上游制品交接

## 2026-11-12 CURRENT：C已修复Android卸载生命周期，双机默认持钥通过

用户明确授权C直接修复AOEM/SUPERVM，本项不再等待A。共享修复`dea182e4`在原bindings
保留Android DSO代码映射到进程退出，防止线程TLS析构晚于dlclose；密钥/上下文释放不变，
Android环境变量不能再开启不安全卸载，Windows/Linux原语义保留。AOEM固定SO/hash未变。
NOVOCHAT `b1d199e`已消费新受审输入；原139和一次不卸载诊断记录完整保留。

两手机默认配置各5项真实communication_custody通过/退出0，命令明确unset卸载环境，
不靠诊断开关通过。两机原pq_preflight及错hash/缺库/错参数拒绝通过。共享bindings
34通过/8原有忽略、策略定向测试和严格Clippy通过，消费者严格Clippy通过。
B首次预检发起遇到ADB daemon不可用未执行签验，恢复连接后只续跑B未完成步骤，
原中断保留，A成功测试不重复。不是App UID/Keystore跨进程验收，也未安装APK或改JNI库pin。

C继续手机App接入，A继续双中继及真实UCA/设备/联系人可信输入，避免重复修本项。
[手机原始结果及失败对照](https://github.com/XujueKing/NOVOCHAT/blob/b1d199e/docs/implementation/ANDROID_FIXED_RUNTIME_2026-11-12.md)。
原聊天身份历史和节点未动，收费关闭；下方等待A修卸载的当前状态由本节覆盖。

## 2026-11-12 CURRENT：用户指定C直接修复共享Android卸载生命周期

用户明确本机同时具备AOEM/SUPERVM源码，本项由validator-C直接修复、构建及手机回归，
覆盖下文等待A修复的分工。A请继续双中继，避免重复修改aoem-bindings卸载策略；
可信UCA/设备/联系人绑定仍由A提供。本次不改AOEM算法或固定输入包。
C针对Android线程退出时DSO/TLS析构回调已卸载的风险，沿原AoemDyn策略保留代码映射
至进程退出；原密钥/上下文Drop仍执行，验签/能力门禁不变。Android不允许环境变量
重新开启不安全卸载；Windows/Linux原策略保留。须以无环境覆盖的原手机测试证明修复。

## 2026-11-12 CURRENT C回传：固定构建成功，默认持钥SIGSEGV交A修共享生命周期

C已执行723b4a45，NOVOCHAT公开回执`86ae1ff`。原ZIP及两项输入hash通过；原脚本
Rust1.94/NDK28.2/API24/release-mldsa退出0。新SO8252616字节，SHA256
`7f283b8152eb30b8873e75be4b764905d37db5c1afaec88238b30146f264bfca`；header
`4c6ae29baf7efc0c8cc1cf3abd0abb78a585a135cb2de47f8ab3fbfdaec969ca`。
AArch64 DYN、libdl/libm/libc及原出口核对通过。固定私有输入/库不上传，原构建日志在C target。

mobile-A原pq_preflight 65/4退出0（算法16负例、共享花费18负例、缺状态拒绝），错hash/
缺库/错参数退出1；但原communication_custody首项线程退出SIGSEGV/139。独立对照
AOEM_FFI_UNLOAD_DLL=0同库同程序5项通过/0；这只是诊断，不替代默认失败。
崩溃顶部未知地址，后接pthread_key_clean_all/pthread_exit；共享bindings默认非Windows
卸载库。A请在原AoemDyn/统一owner核对动态库卸载与TLS析构生命周期，修原实现并回归；
C没有改默认Drop或以环境开关签收。不能把该线索写成已经完全归因。

两机在线但B尚未测试；已停止APK/Keystore后续，新库未写JNI pin，聊天包/身份历史未动。
[原始命令、退出码、限定本进程崩溃及对照](https://github.com/XujueKing/NOVOCHAT/blob/86ae1ff/docs/implementation/ANDROID_FIXED_RUNTIME_2026-11-12.md)。
默认资格仍未通过，App UID持钥/手机生命周期/正式收费均未完成。下方输入构建指令保留，
不用重传相同ZIP，也不重建相同运行库；待共享生命周期修复后C续验。

## 2026-11-12 CURRENT：Android 构建与手机验证转交 C，A 回双中继主线

用户明确要求复用 C 已有 AOEM、NDK 和手机环境。**C 负责固定输入构建、原 JNI／
验签／Keystore 实测；A 负责共享实现与接口缺陷，不再并行维护手机开发线。**
不等待 A 完成 Android 编译，也不让 C 重写共享签名或网络。A 的 r2 已进入 Rust
编译，因本次移交主动结束；没有生成可签收的新 Android SO，不能计为编译通过或
源代码编译失败。r1 的 Windows tar 中文路径失败及 r2 日志保留在 A 本地 target。
旧 5b08 库缺少当前统一批量验签能力的拒绝仍有效；下方双手机通过仅属旧接口历史。

### C 现在执行（无需等 A 的 Android 构建）

1. 同步本条所在 SUPERVM main，复用 `scripts/build-aoem-android-runtime.ps1`。
   NOVOCHAT 继续由 C 开发；本条不修改其源码或开启收费。
2. 核对本机 AOEM 包含提交 `4c57372ad0cc12123df5ca9e163a130039bfb599`。
   脚本用 Git 导出固定提交到新目录，不覆盖 C 的 AOEM 工作区；不把 C 的未提交
   改动当作固定输入。如缺提交，通过原私有 AOEM 远端取得，不另造源码替代。
3. 取得 A 的小型私有输入包（不是新的 APK／SO，也不含密钥）：
   `C:\Users\leadb\AppData\Local\NOVOVM\android-runtime-handoff-20261112\android-aoem-fixed-inputs-v1.zip`。
   **1,252,757 字节**，SHA256
   `f7d2069493669a10c257c13eaedb55c4843b8a451fd41fae268e1d0e0f477970`。
   复制到 C 原接收目录后核验。若 C 已有以下两个完全同哈希输入，可直接复用，
   不必复制。包内仅 `source-overlay.tar.gz` 和 `Cargo.lock`；属于 AOEM 私有源码
   输入，不提交到公共 SUPERVM/NOVOCHAT 仓库。
4. 两项输入分别核验 SHA256：overlay
   `8ee88c113611cbdcfacce949ae06b49f9870bb8cdd570a8eddeb8343505ab4fb`；lock
   `46625b6be8ad25c323fbba6e23263fe9f9f1b649805303ebdb89a7fe7f7abbec`。
   来源复用既有 Windows/Linux native65 固定快照；不是仅凭 base commit 即包含
   全部补丁。不得以 C 当前不同的 lock／源码替换。
5. 使用已有 Rust 1.94.0、NDK28.2.13676358/API24、ARM64 target、GLSLC。
   在 SUPERVM 根目录运行（路径按 C 实际位置填写，OutputRoot 必须是新目录）：

```powershell
& ./scripts/build-aoem-android-runtime.ps1 `
  -AoemRepository 'D:\WEB3_AI\AOEM' `
  -SourceOverlay 'E:\区块链四机通讯\android-aoem-fixed-inputs-v1\source-overlay.tar.gz' `
  -QualifiedLock 'E:\区块链四机通讯\android-aoem-fixed-inputs-v1\Cargo.lock' `
  -NdkRoot 'D:\Android\Sdk\ndk\28.2.13676358' `
  -OutputRoot 'D:\WEB3_AI\SUPERVM\target\android-aoem-C-20261112-v1'
```

脚本复用原 `--locked --release -p aoem-ffi --no-default-features --features mldsa`
移动合同，保留原实现、许可与权限检查，不是 desktop fullmax 或主节点包。
其末端封包与 ELF 检查尚未在 A 完整跑到，C 须按实际结果报告，不能称脚本全程已验收。

### C 回传与验收边界

- 先回传构建退出码／日志、candidate-manifest、库/header 哈希、ELF 架构、依赖
  与原出口。失败回传首因，不关闭必需检查；若属共享源码缺陷，交 A 修原实现。
- 构建通过后复用 C 原 `pq_preflight`，独立 pin 新库 SHA256，保留已知向量、
  篡改、错哈希、缺库、参数等拒绝；确认原统一语义验签实际执行，不用标量旧结果顶替。
- 再沿原 `MldsaRuntime`、`verification_owner_v1`、`from_mldsa` 和 C 已有 JNI／
  Store／identity worker 验证。JNI 的旧运行库 pin 只能绑定实际已核验新库，不能删检查。
- 临时 adb shell 签验与 App UID 下 Keystore enroll／重启／同钥恢复分开；旧身份
  连续性、真实 UCA／设备授权仍需原可信输入，不填恒真状态，不以新临时钥冒充注册。
- 完成后在原网络交接追加 C 回执、NOVOCHAT 提交与公开结果，不上传私钥、完整库、
  私有 AOEM 补丁或用户数据。构建通过不自动更新正式 SDK manifest／安装聊天包。

A 已做的检查和失败边界见
[运行库输入记录](../artifacts/audit/android-mldsa-runtime-20261112/README.md)。
Android 任务**不是四机双中继部署前置**；固定四机程序仍为 a25452f4。

日期2026-10-24；SUPERVM/经济侧唯一维护共享接口，Mac可作为同源码构建执行端。
用户确认有Mac与Xcode Cloud；尚未取得该执行端的构建记录或制品。
本文件记录构建起点、后续候选实机结果及资格边界。**Android 候选已在双手机隔离签验通过；正式 SDK 发布、App 持久持钥和可信资产执行尚未交付。** iPhone 制品仍未交付。下方原始缺口描述保留为构建前记录，以最新结果为准。

## 最新结果：Android 候选双机隔离签验通过

NOVOCHAT 构建执行端按本页指令，在干净 AOEM `6d590969`、NDK
28.2.13676358/API24、`aarch64-linux-android` 下完成 release/mldsa 构建；
无默认 features，未修改共享算法/ABI。候选 `libaoem_ffi.so` 的 SHA-256 为
`5b08e48d4ed66893bdc7b65ec4c65e60d6fd67a58c6efa178989f857d798ac9a`。
ELF 为 AArch64 DYN，动态依赖仅 libdl/libm/libc，原 keygen/sign/verify/free 导出存在。
制品仍是本机隔离候选，未写入正式 SDK manifest 或提交二进制。

PKL110/Android15 与 PCLM50/Android12 各用原 App 消费探针和临时密钥完成：
4个裸算法样本/16项反例，共享规范花费对象签验/编码及8项拒绝检查
（含未知/零值/旧授权代次），另有错核心哈希、缺库、错误参数3项入口拒绝。
asset_authorized=false、transaction_submitted=false。临时目录执行后定向清理；
不安装 APK、不读写 App 身份历史，v108 包哈希和原进程前后均保持。

正式证据由 [App 手机签验记录](../../NOVOCHAT/docs/implementation/UCA_PQ_ANDROID_DEVICE_V1.md)
与其原始 JSON 提供；不能从 adb shell 下的临时持钥扩展为 App Keystore、硬件
持钥或持久恢复通过。候选的正式平台装载/分发、完整内存清理资格及应用持钥仍须
接续验收；本轮没有原子额度/nonce/余额/费用提交、主链到账或收费结果。

## Android 构建执行端接续（2026-10-24）

NOVOCHAT 机器已具备本页指定 NDK/API、Rust target 和 GLSLC，按下方既有命令
承担本次构建执行与双手机隔离消费验证。共享算法、ABI、账户/资产执行及运行库
发布资格仍由 SUPERVM/AOEM 主责；本机不复制或修改签名实现，不另建协议。
另一台无需为这项手机验证重复开发包装或运行库。

最初本机 AOEM 停在 `1061a5ee`，构建因缺少
`word256_row_arithmetic_v0.spv` 失败；核对已有修复记录后，已将干净源码快进至
本页原指定的 `6d590969` 重建，未自行修改 GPU/密码源码或跳过着色器构建。
该修复与首份失败日志保留。此处原为执行认领，手机结果现见本页上方最新节及
[App 接入记录](../../NOVOCHAT/docs/implementation/UCA_ANDROID_PQ_INTEGRATION_V1.md)
按源码、实际制品和手机证据单独回填。仍不启用真实收费。

## 现成源码与实际缺口

只读核查 AOEM `6d590969a1bf688306d663bf1c45e8213af92ce0`：

- `crates/ffi/aoem-ffi` 已提供C ABI、cdylib/rlib；`mldsa` feature复用
  `aoem-adapter-crypto/native/mldsa_bridge.c` 的现有ARM portable实现，无需新密码算法。
- keygen/sign需要核心编入 `mldsa`；仅配verify sidecar不满足钱包签名。该feature还
  引入现有SPIR-V编译依赖，不能假称完全不需要shader构建工具。
- header使用 `crates/ffi/aoem-ffi/include/aoem.h`，保留实际ABI与缓冲区配对释放。
- 当前SUPERVM manifest只有Windows/Linux x86_64；无合格Android/iOS条目。
  首次交接核查的上游主机只有Windows Rust target，无NDK/Apple SDK；当时D盘剩余约4.5GiB，
  未自动下载大工具链或清理证据。Mac/Xcode Cloud可承担Apple平台构建。
- 既有桌面默认库查找将移动端当作Linux；Android先用明确库路径，后续须补真实
  平台manifest/loader。iPhone静态链接需要同一bindings的显式适配，`AoemDyn::load`
  不能拿静态`.a`路径当动态库加载。不得由App复制一套密码包装来补此缺口。

## Mac 构建执行端任务（复制给该机 Codex）

先同步SUPERVM交接版本和上述AOEM固定源码，保留已有修改；只在独立构建输出中工作，
不改共享签名字节或开发App功能。回传源提交、工具版本、失败首因和实际产物哈希。
以下命令为源代码核对后的起点，**尚未在Mac执行验证**，失败必须保留，不删除功能绕过。

```bash
# 在SUPERVM根目录；AOEM_SOURCE指向已有AOEM工作树。需已有受信Rust工具链和glslc。
export AOEM_SOURCE="../AOEM"
git -C "$AOEM_SOURCE" rev-parse HEAD
git -C "$AOEM_SOURCE" status --short
xcodebuild -version
xcrun --sdk iphoneos --show-sdk-path
xcrun --sdk iphonesimulator --show-sdk-path
rustup target list --installed
command -v glslc

# 缺少时由构建端安装这两个标准库目标；不改算法、Cargo特性或签名合同。
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
export AOEM_GLSLC_PATH="$(command -v glslc)"
export CARGO_TARGET_DIR="$PWD/target/aoem-mobile-ios"
cargo rustc --manifest-path "$AOEM_SOURCE/Cargo.toml" -p aoem-ffi --lib \
  --locked --release --no-default-features --features mldsa \
  --target aarch64-apple-ios --crate-type staticlib
cargo rustc --manifest-path "$AOEM_SOURCE/Cargo.toml" -p aoem-ffi --lib \
  --locked --release --no-default-features --features mldsa \
  --target aarch64-apple-ios-sim --crate-type staticlib
```

静态库编译通过以后才整理device/simulator两个slice、原C header及XCFramework；
不能把macOS库或模拟器slice标为iPhone。之后仍需上游统一bindings静态适配、App
链接/签名、真机调用与内存释放测试。Xcode Cloud沿同一固定源码/构建命令执行，
不另建移动专用AOEM算法。不要上传私钥或签票/用户数据库。

## Android 构建执行端任务

先提供NDK28.2.13676358/API24及Rust `aarch64-linux-android`；以下工具链前缀依宿主
选Windows/Linux/macOS，输出都是Android Bionic ARM64，非宿主Linux库。

```text
CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=<NDK>/toolchains/llvm/prebuilt/<host>/bin/aarch64-linux-android24-clang[.cmd]
CC_aarch64_linux_android=<same compiler>
AR_aarch64_linux_android=<NDK>/toolchains/llvm/prebuilt/<host>/bin/llvm-ar[.exe]
AOEM_GLSLC_PATH=<existing host glslc>
cargo build --manifest-path <AOEM>/Cargo.toml --locked --release -p aoem-ffi --no-default-features --features mldsa --target aarch64-linux-android --target-dir <SUPERVM>/target/aoem-mobile-android
```

产物应为该target/release/libaoem_ffi.so。核ELF架构、依赖、实际导出、ABI、核心哈希及
是否仍加载插件；有插件则逐一列身份。不能只通过core哈希就说所有代码可信。
此构建调用已有AOEM源码。原交接时未产生Android库；后续实际候选及双机结果见上方最新节，不能以消费者ELF代替运行库。

## 同一接口与资格

App Rust侧消费 `novovm-protocol::uca_spend` 与
`novovm-exec::AoemExecFacade::{uca_spend_signer_v1,uca_spend_verifier_v1}`。
规范输入、测试向量及未接通状态见[签名合同](NOVOVM_FIRST_RELEASE_SIGNATURE_CONTRACT.md)。
暂时不能将签名对象提交为当前原节点V3交易；可信UCA状态消费仍在上游待办。

每个平台分别给出：源码提交/目标/API与工具链、库/header/插件哈希、实际ABI和能力、
65的1952/4032/3309尺寸、官方向量及篡改反例、真实规范意图签验、缓冲区释放、
缺库/错库/错误参数拒绝结果。真机持钥和App升级由NOVOCHAT侧验，保留v108及身份历史。
运行库可加载、密码测试、可信花费授权、主链到账再花、性能是独立验收项。
