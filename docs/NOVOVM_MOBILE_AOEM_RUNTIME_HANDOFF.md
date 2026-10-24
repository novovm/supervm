# Android / iPhone AOEM 上游制品交接

日期2026-10-24；SUPERVM/经济侧唯一维护共享接口，Mac可作为同源码构建执行端。
用户确认有Mac与Xcode Cloud；尚未取得该执行端的构建记录或制品。
本文件是可执行的构建起点及资格边界，**不是已交付运行库或手机通过**。

## 现成源码与实际缺口

只读核查 AOEM `6d590969a1bf688306d663bf1c45e8213af92ce0`：

- `crates/ffi/aoem-ffi` 已提供C ABI、cdylib/rlib；`mldsa` feature复用
  `aoem-adapter-crypto/native/mldsa_bridge.c` 的现有ARM portable实现，无需新密码算法。
- keygen/sign需要核心编入 `mldsa`；仅配verify sidecar不满足钱包签名。该feature还
  引入现有SPIR-V编译依赖，不能假称完全不需要shader构建工具。
- header使用 `crates/ffi/aoem-ffi/include/aoem.h`，保留实际ABI与缓冲区配对释放。
- 当前SUPERVM manifest只有Windows/Linux x86_64；无合格Android/iOS条目。
  本机只有Windows Rust target，无NDK/Apple SDK；D盘本次核查剩余约4.5GiB，
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
此构建调用已有AOEM源码；本轮未产生Android库，不以另一机消费者ELF代替。

## 同一接口与资格

App Rust侧消费 `novovm-protocol::uca_spend` 与
`novovm-exec::AoemExecFacade::{uca_spend_signer_v1,uca_spend_verifier_v1}`。
规范输入、测试向量及未接通状态见[签名合同](NOVOVM_FIRST_RELEASE_SIGNATURE_CONTRACT.md)。
暂时不能将签名对象提交为当前原节点V3交易；可信UCA状态消费仍在上游待办。

每个平台分别给出：源码提交/目标/API与工具链、库/header/插件哈希、实际ABI和能力、
65的1952/4032/3309尺寸、官方向量及篡改反例、真实规范意图签验、缓冲区释放、
缺库/错库/错误参数拒绝结果。真机持钥和App升级由NOVOCHAT侧验，保留v108及身份历史。
运行库可加载、密码测试、可信花费授权、主链到账再花、性能是独立验收项。
