# WEB3006: 去中心化存储接口标准

**版本**: v0.1.0  
**状态**: Draft  
**作者**: SuperVM Core Team  

---

## 设计理念

WEB3006 是 SuperVM 与 IPFS/Arweave/Filecoin 等存储网络的统一接口，支持加密存储、跨链同步。

## 核心创新

| 传统存储接口 | **WEB3006** |
|------------|-------------|
| 单一存储后端 | ✅ **多后端聚合（IPFS/Arweave）** |
| 明文存储 | ✅ **加密存储（环签名密钥）** |
| 单链访问 | ✅ **跨链访问控制** |

---

## Rust Trait 接口

```rust
#[async_trait::async_trait]
pub trait WEB3006Storage {
    // ============ 上传 ============
    
    /// 上传文件到去中心化存储
    async fn upload(
        &self,
        data: Vec<u8>,
        backend: StorageBackend,
        encryption: Option<EncryptionKey>,
    ) -> Result<ContentHash, StorageError>;
    
    /// 上传大文件（分片）
    async fn upload_chunked(
        &self,
        chunks: Vec<Vec<u8>>,
        backend: StorageBackend,
    ) -> Result<ContentHash, StorageError>;
    
    // ============ 下载 ============
    
    /// 下载文件
    async fn download(
        &self,
        content_hash: ContentHash,
        backend: StorageBackend,
        decryption_key: Option<EncryptionKey>,
    ) -> Result<Vec<u8>, StorageError>;
    
    // ============ 访问控制 ============
    
    /// 授权访问（基于 NFT 或 DID）
    async fn grant_access(
        &self,
        content_hash: ContentHash,
        grantee: Address,
        expires_at: Option<u64>,
    ) -> Result<TransactionHash, StorageError>;
    
    /// 撤销访问
    async fn revoke_access(
        &self,
        content_hash: ContentHash,
        grantee: Address,
    ) -> Result<TransactionHash, StorageError>;
    
    // ============ 跨链同步 ============
    
    /// 跨链同步文件引用
    async fn sync_cross_chain(
        &self,
        content_hash: ContentHash,
        target_chains: Vec<ChainId>,
    ) -> Result<Vec<TransactionHash>, StorageError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageBackend {
    IPFS,
    Arweave,
    Filecoin,
    Custom(String),
}

pub type ContentHash = String;  // IPFS CID or Arweave TX ID
```

---

## 应用场景

### **NFT 元数据存储**
```rust
// 上传 NFT 图片到 IPFS（加密）
let cid = storage.upload(
    image_data,
    StorageBackend::IPFS,
    Some(encryption_key),
).await?;

// 铸造 NFT 时引用 CID
nft.mint(owner, token_id, format!("ipfs://{}", cid), None).await?;
```

---

## Roadmap

| 阶段 | 功能 | 状态 |
|------|------|------|
| **Phase 1** | IPFS 集成 | 📋 设计中 |
| **Phase 2** | Arweave 集成 | 📋 规划中 |
| **Phase 3** | 加密存储 | 📋 规划中 |
| **Phase 4** | 跨链同步 | 📋 规划中 |
