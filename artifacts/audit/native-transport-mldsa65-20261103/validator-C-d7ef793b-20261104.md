# validator-C：d7ef793b Windows 独立候选接收与本机核验

日期：2026-11-04（Asia/Shanghai）；节点 **validator-C**。状态：**包验真、本机运行库资格、独立传输用途身份准备与两次只读 inspect 均通过；未做跨机认证、未启链。**

## 接收与版本

- 已从原 main `6cbf0177` 无损快进至交接提交 **`331a55c6c728bd98b53c778de8a9b1eb245fe782`**，阅读根 AGENTS.md、交付对齐顶部2026-11-04指令、共同配置及用途身份准备合同。
- 实际程序源码固定 **`d7ef793b819c8670e472d217daae9db5f074aea8`**；target **`x86_64-pc-windows-msvc`**。本回执不是新程序构建。
- 收到ZIP：`E:/native65-d7ef793b-windows-x86_64-20261104.zip`，**144558493字节**。
- 独立解压目录：`C:/Users/leadb/AppData/Local/NOVOVM/candidates/native65-d7ef793b-windows-x86_64`。执行前确认目录不存在，未覆盖/清理其他目录；未运行安装器，未请求管理员权限。

| 固定对象 | C实际SHA256（与A公布值一致） |
| --- | --- |
| ZIP | `d025e678f793fffc95a22b21be6b7aeb8d0aa369e5e0ba6df63ba4c06e2238cc` |
| release-manifest.json | `f6caccf81d205974102d34c00300ff10814de5ff13aea8c041d6f7a00cf3eeb4` |
| CHECKSUMS.sha256 | `301e95eb56419e90e882562448d2aedc4c30e0e89343bf0db4669721ce2a09f8` |
| 实际AOEM core | `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50` |

逐项核验manifest的**80项artifacts**大小及SHA256，同时与根CHECKSUMS匹配；**11个原构建合同**的实际EXE哈希匹配。实际83个普通文件由80项制品和3份清单元数据组成，全部可解释。额外嵌套`aoem/windows/CHECKSUMS.sha256`的**26条**也逐项验真。运行资格检查和身份检查后再次核对80项大小/哈希，候选文件保持不变。

本地首版校验脚本误把嵌套CHECKSUMS当额外制品而触发AssertionError，当时已停止且未执行程序。核对原打包器按文件名排除CHECKSUMS的规则后，修正本地核验的文件集合并补验26条；未修改包、manifest或源码，未绕过任何制品哈希检查。

## C实际运行库资格

只执行候选中的`bin/novovmctl.exe native-identity qualify-runtime`，显式指定同一候选的`aoem/windows/core/bin/aoem_ffi.dll`及上述固定哈希；未用PATH旧程序、正式随包库或源码重编译产物。

实际加载路径（CLI canonical输出）：`\\?\C:\Users\leadb\AppData\Local\NOVOVM\candidates\native65-d7ef793b-windows-x86_64\aoem\windows\core\bin\aoem_ffi.dll`。

退出码**0**、`ok=true`，以下均true：`verification_qualified`、`scalar_verification_kat_passed`、`unified_semantic_verification_kat_passed`、`mixed_batch_verification_kat_passed`。报告中的`signing_tested=false`和`keygen_tested=false`保留：该资格入口本身只做验签KAT及签名接口尺寸检查；后续用途身份操作独立记录，不能混写为全部算法/场景验收。

## 本机传输身份

按指令先确认指定域`transport.sealed`不存在，才在原约定保护目录**新建本机独立身份**；没有从A/B/D复制，也没有更换旧身份。仅新私有目录限制为当前用户、SYSTEM和Administrators，生成/检查均在同一Windows用户上下文运行，复用原DPAPI current-user保护。

| 字段 | C实际公开结果 |
| --- | --- |
| purpose | `transport` |
| chain_id | `167999907280566`（本次公测传输域，非正式主网值） |
| algorithm / envelope schema | `ML-DSA-65` / `3` |
| 公钥长度 | **1952字节** |
| 公钥SHA256（原始公钥字节，不是hex文本） | **`a3760975c77a717ad64e9d18c95f1f6b4d043298b9f359aeaf9fe61687355b2f`** |
| 完整公钥 | [公开身份JSON的public_key_hex](validator-C-d7ef793b-20261104-identity.json)；hex解码后可直接用于对应规范公钥输入 |
| 操作 | 一次generate退出0/ok=true；随后两个独立CLI进程inspect均退出0/ok=true |
| 一致性 | 三次公钥/用途/域完全一致，inspect前后sealed文件长度及mtime不变 |
| 私钥/credential导出 | **false**；未上传sealed、wrapping credential、口令或令牌 |

公开指纹不同于A公布的`0656e6de01e498b50d7b67b10ff58f4c600cdf61663066b03f358a7901839eae`。这只是已知公钥对照，不把独立生成或本机inspect等同于全网独立性证明。最终Windows服务运行账户尚未配置/验收，后续必须遵守原DPAPI账户绑定，不能为换账户静默重建该身份。

## 公开证据与给A的回传

| 公开证据 | Git/LF字节SHA256 |
| --- | --- |
| [validator-C-d7ef793b-20261104-package.json](validator-C-d7ef793b-20261104-package.json) | `2f269298dff5d5479b247e173f4621d8ba30d565b4bf3b92f077be8b6e54f3d6` |
| [validator-C-d7ef793b-20261104-runtime.json](validator-C-d7ef793b-20261104-runtime.json) | `0a6d57577bdc3dd9edc8632080e39a3a03a32a493a15cb0ca316cbe26933201b` |
| [validator-C-d7ef793b-20261104-identity.json](validator-C-d7ef793b-20261104-identity.json) | `08c8a0e63b7a8c837353d2f953fca2687202f2b2d14b31306e8ba386a1d4f171` |

A请接收本回执及公开公钥，回传`received_commit / report_sha256 / transport_public_key_sha256 / next_authorized_phase`。Git推送不等于A已阅读。由A汇总原认证配置后再组织双向身份、出口、重连和旧会话拒绝验收；本次没有启动NAT观察、守护程序或验证者，也没有执行普通UDP echo冒充原认证。

本次没有重新编译、安装/启停服务、重做基础机器清点或校时，没有生成validator钥、创世或NOV，没有修改现行身份、服务、数据库、正式AOEM库、手机聊天身份历史或生产收费。临时公测传输身份不提供验证者资格、资产权限、完整抗量子密钥协商、四机通信或TPS资格。
