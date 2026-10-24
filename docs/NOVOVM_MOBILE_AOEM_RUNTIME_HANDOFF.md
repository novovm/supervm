# Android / iPhone AOEM 上游制品交接

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
