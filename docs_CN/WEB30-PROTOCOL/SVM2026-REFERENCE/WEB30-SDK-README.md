# WEB30 SDK 使用指南

完整的 TypeScript SDK，用于与 WEB30 协议族交互。

## 安装

```bash
npm install @supervm/web30
# 或
yarn add @supervm/web30
```

## 快速开始

### 基础代币操作

```typescript
import { SuperVMClient, parseTokenAmount } from '@supervm/web30';

// 初始化客户端
const client = new SuperVMClient({
  rpcUrl: 'http://localhost:8545',
  privateKey: 'your-private-key' // 或使用 MetaMask
});

// 获取代币实例
const token = client.getToken('0xTokenAddress...');

// 转账
const receipt = await token.transfer(
  '0xRecipient...',
  parseTokenAmount('100', 18)
);
console.log('Transfer successful:', receipt.txHash);
```

### 跨链转账

```typescript
const crossReceipt = await token.transferCrossChain(
  137, // Polygon chain ID
  '0xRecipient...',
  parseTokenAmount('50', 18)
);

console.log('Swap ID:', crossReceipt.swapId);
console.log('Status:', crossReceipt.status); // 'pending'
```

### 批量转账

```typescript
const recipients = ['0xAddr1...', '0xAddr2...', '0xAddr3...'];
const amounts = ['100', '200', '300'].map(amt => parseTokenAmount(amt, 18));

const receipts = await token.batchTransfer(recipients, amounts);
console.log(`Sent ${receipts.length} transfers in one tx`);
```

## WEB3005 身份与 KYC

### 统一账户

```typescript
// 查询账户信息
const account = await client.identity.getAccount(myAddress);
console.log('Numeric ID:', account.numericId); // "123-456-789-012"
console.log('External Wallets:', account.externalWallets);
```

### 绑定外部钱包

```typescript
await client.identity.bindWallet(
  myAddress,
  900, // Solana
  'solana',
  'SolanaAddress...',
  signatureProof
);
```

### 登录流程

```typescript
// 1. 请求挑战
const challenge = await client.identity.requestLoginChallenge(myAddress);

// 2. 签名挑战
const signature = await signer.signMessage(challenge.challenge);

// 3. 验证并获取 token
const loginToken = await client.identity.login(
  myAddress,
  challenge.challenge,
  signature
);

console.log('Token:', loginToken.token);
```

### KYC 零知识证明

```typescript
// 生成证明（前端）
const zkProof = await client.identity.proveKycLevel({
  account: myAddress,
  level: 'standard',
  challenge: serverChallenge
});

// 验证证明（服务端）
const isValid = await client.identity.verifyKycProof({
  proof: zkProof,
  level: 'standard',
  attestors: [{ id: 'did:svm:attestor:bankX', policy: ['standard'] }],
  challenge: serverChallenge
});
```

## API 参考

### SuperVMClient

主客户端类，提供对所有 WEB30 协议的访问。

```typescript
class SuperVMClient {
  constructor(config: ClientConfig);
  getToken(address: string): WEB30TokenClient;
  identity: WEB3005Client;
  getAddress(): Promise<string>;
  getBalance(address?: string): Promise<string>;
}
```

### WEB30TokenClient

WEB30 代币标准客户端。

```typescript
class WEB30TokenClient {
  // 基础信息
  name(): Promise<string>;
  symbol(): Promise<string>;
  decimals(): Promise<number>;
  totalSupply(): Promise<string>;
  
  // 余额
  balanceOf(account: Address): Promise<string>;
  
  // 转账
  transfer(to: Address, amount: string): Promise<TransferReceipt>;
  batchTransfer(recipients: Address[], amounts: string[]): Promise<TransferReceipt[]>;
  
  // 授权
  approve(spender: Address, amount: string): Promise<void>;
  allowance(owner: Address, spender: Address): Promise<string>;
  transferFrom(from: Address, to: Address, amount: string): Promise<TransferReceipt>;
  
  // 高级功能
  mint(to: Address, amount: string): Promise<void>;
  burn(amount: string): Promise<void>;
  freeze(account: Address): Promise<void>;
  unfreeze(account: Address): Promise<void>;
  
  // 跨链
  transferCrossChain(toChain: number, toAddress: Address, amount: string): Promise<CrossChainReceipt>;
  
  // 隐私
  transferPrivate(stealthAddress: StealthAddress, amount: string, ringSignature: RingSignature): Promise<void>;
  
  // 元数据
  getMetadata(): Promise<Partial<TokenMetadata>>;
}
```

### WEB3005Client

身份与 KYC 管理客户端。

```typescript
class WEB3005Client {
  // 账户管理
  getAccount(accountId: Address | string): Promise<UnifiedAccount | null>;
  bindWallet(account: Address, chainId: number, chainType: string, externalAddress: string, signature: string): Promise<void>;
  unbindWallet(account: Address, chainId: number, externalAddress: string, signature: string): Promise<void>;
  
  // 登录
  requestLoginChallenge(account: Address): Promise<LoginChallenge>;
  login(account: Address, challenge: string, signature: string): Promise<LoginToken>;
  
  // KYC
  getKYCStatus(account: Address): Promise<KYCStatus>;
  proveKycLevel(params: ProveKycLevelParams): Promise<ZKProof>;
  verifyKycProof(params: VerifyKycProofParams): Promise<boolean>;
  getKYCCredential(account: Address, authToken: string): Promise<KYCCredential | null>;
}
```

## 工具函数

### 代币金额格式化

```typescript
import { formatTokenAmount, parseTokenAmount } from '@supervm/web30';

// 解析（人类可读 → 最小单位）
const amount = parseTokenAmount('123.45', 18);
// '123450000000000000000'

// 格式化（最小单位 → 人类可读）
const readable = formatTokenAmount('123450000000000000000', 18);
// '123.45'
```

### 数字账户格式化

```typescript
import { formatNumericId, parseNumericId } from '@supervm/web30';

const formatted = formatNumericId('123456789012');
// '123-456-789-012'

const parsed = parseNumericId('123-456-789-012');
// '123456789012'
```

### KYC 等级比较

```typescript
import { meetsKycLevel } from '@supervm/web30';

const canAccess = meetsKycLevel('standard', 'basic');
// true (standard >= basic)

const cannotAccess = meetsKycLevel('basic', 'high');
// false (basic < high)
```

## 完整示例

参见 `examples/` 目录：
- `simple-transfer.ts` - 基础转账与跨链
- `kyc-workflow.ts` - 身份登录与 KYC 流程

## 错误处理

```typescript
try {
  await token.transfer(recipient, amount);
} catch (error) {
  if (error.message.includes('insufficient balance')) {
    console.error('余额不足');
  } else if (error.message.includes('frozen')) {
    console.error('账户已冻结');
  } else {
    console.error('转账失败:', error);
  }
}
```

## 许可证

MIT
