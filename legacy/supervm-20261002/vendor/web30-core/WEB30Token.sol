// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title WEB30 Token Standard
/// @notice Next-generation token standard with MVCC parallelism, cross-chain, and privacy
/// @dev Reference implementation for EVM compatibility
interface IWEB30 {
    // ========== Events ==========
    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event CrossChainTransfer(uint64 indexed toChain, address indexed to, uint256 value, bytes32 swapId);
    event PrivateTransfer(bytes indexed stealthAddress, uint256 value);
    event AccountFrozen(address indexed account);
    event AccountUnfrozen(address indexed account);
    event ProposalCreated(uint256 indexed proposalId, address indexed proposer);
    event Voted(uint256 indexed proposalId, address indexed voter, bool support, uint256 votes);

    // ========== Basic Info ==========
    function name() external view returns (string memory);
    function symbol() external view returns (string memory);
    function decimals() external view returns (uint8);
    function totalSupply() external view returns (uint256);

    // ========== Balance Query ==========
    function balanceOf(address account) external view returns (uint256);

    // ========== Transfer Operations ==========
    function transfer(address to, uint256 amount) external returns (bool);
    function batchTransfer(address[] calldata recipients, uint256[] calldata amounts) external returns (bool);

    // ========== Allowance & Delegation ==========
    function allowance(address owner, address spender) external view returns (uint256);
    function approve(address spender, uint256 amount) external returns (bool);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);

    // ========== Advanced Features ==========
    function mint(address to, uint256 amount) external;
    function burn(uint256 amount) external;
    function freeze(address account) external;
    function unfreeze(address account) external;

    // ========== Cross-Chain Extension ==========
    function transferCrossChain(
        uint64 toChain,
        address toAddress,
        uint256 amount
    ) external returns (bytes32 swapId);

    // ========== Privacy Extension ==========
    function transferPrivate(
        bytes calldata stealthAddress,
        uint256 amount,
        bytes calldata ringSignature
    ) external returns (bool);

    // ========== Governance Extension ==========
    function propose(bytes calldata proposalData) external returns (uint256 proposalId);
    function vote(uint256 proposalId, bool support, uint256 voteAmount) external;
    function execute(uint256 proposalId) external;

    // ========== Metadata ==========
    function metadata() external view returns (
        string memory iconUri,
        string memory description,
        string memory website
    );
}

/// @title WEB30Token
/// @notice Reference implementation of WEB30 Token Standard
contract WEB30Token is IWEB30 {
    // ========== State Variables ==========
    string private _name;
    string private _symbol;
    uint8 private _decimals;
    uint256 private _totalSupply;

    mapping(address => uint256) private _balances;
    mapping(address => mapping(address => uint256)) private _allowances;
    mapping(address => bool) private _frozen;

    address private _owner;
    mapping(address => bool) private _minters;

    // Metadata
    string private _iconUri;
    string private _description;
    string private _website;

    // Governance
    uint256 private _proposalCount;
    mapping(uint256 => Proposal) private _proposals;
    mapping(uint256 => mapping(address => bool)) private _hasVoted;

    struct Proposal {
        address proposer;
        string title;
        string description;
        uint256 startTime;
        uint256 endTime;
        uint256 votesFor;
        uint256 votesAgainst;
        ProposalStatus status;
        bytes actions;
    }

    enum ProposalStatus {
        Pending,
        Active,
        Succeeded,
        Failed,
        Executed
    }

    // ========== Modifiers ==========
    modifier onlyOwner() {
        require(msg.sender == _owner, "WEB30: caller is not owner");
        _;
    }

    modifier onlyMinter() {
        require(_minters[msg.sender], "WEB30: caller is not minter");
        _;
    }

    modifier notFrozen(address account) {
        require(!_frozen[account], "WEB30: account is frozen");
        _;
    }

    // ========== Constructor ==========
    constructor(
        string memory name_,
        string memory symbol_,
        uint8 decimals_,
        uint256 initialSupply
    ) {
        _name = name_;
        _symbol = symbol_;
        _decimals = decimals_;
        _owner = msg.sender;
        _minters[msg.sender] = true;

        if (initialSupply > 0) {
            _mint(msg.sender, initialSupply);
        }
    }

    // ========== Basic Info ==========
    function name() external view override returns (string memory) {
        return _name;
    }

    function symbol() external view override returns (string memory) {
        return _symbol;
    }

    function decimals() external view override returns (uint8) {
        return _decimals;
    }

    function totalSupply() external view override returns (uint256) {
        return _totalSupply;
    }

    // ========== Balance Query ==========
    function balanceOf(address account) public view override returns (uint256) {
        return _balances[account];
    }

    // ========== Transfer Operations ==========
    function transfer(address to, uint256 amount)
        external
        override
        notFrozen(msg.sender)
        notFrozen(to)
        returns (bool)
    {
        _transfer(msg.sender, to, amount);
        return true;
    }

    function batchTransfer(address[] calldata recipients, uint256[] calldata amounts)
        external
        override
        notFrozen(msg.sender)
        returns (bool)
    {
        require(recipients.length == amounts.length, "WEB30: array length mismatch");

        uint256 totalAmount = 0;
        for (uint256 i = 0; i < amounts.length; i++) {
            totalAmount += amounts[i];
        }
        require(_balances[msg.sender] >= totalAmount, "WEB30: insufficient balance");

        for (uint256 i = 0; i < recipients.length; i++) {
            require(!_frozen[recipients[i]], "WEB30: recipient is frozen");
            _transfer(msg.sender, recipients[i], amounts[i]);
        }

        return true;
    }

    // ========== Allowance & Delegation ==========
    function allowance(address owner, address spender) public view override returns (uint256) {
        return _allowances[owner][spender];
    }

    function approve(address spender, uint256 amount) external override returns (bool) {
        _approve(msg.sender, spender, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount)
        external
        override
        notFrozen(from)
        notFrozen(to)
        returns (bool)
    {
        uint256 currentAllowance = _allowances[from][msg.sender];
        require(currentAllowance >= amount, "WEB30: insufficient allowance");

        _transfer(from, to, amount);
        _approve(from, msg.sender, currentAllowance - amount);

        return true;
    }

    // ========== Advanced Features ==========
    function mint(address to, uint256 amount) external override onlyMinter {
        _mint(to, amount);
    }

    function burn(uint256 amount) external override {
        _burn(msg.sender, amount);
    }

    function freeze(address account) external override onlyOwner {
        _frozen[account] = true;
        emit AccountFrozen(account);
    }

    function unfreeze(address account) external override onlyOwner {
        _frozen[account] = false;
        emit AccountUnfrozen(account);
    }

    // ========== Cross-Chain Extension ==========
    function transferCrossChain(
        uint64 toChain,
        address toAddress,
        uint256 amount
    ) external override notFrozen(msg.sender) returns (bytes32 swapId) {
        require(_balances[msg.sender] >= amount, "WEB30: insufficient balance");

        // Lock tokens on this chain
        _balances[msg.sender] -= amount;

        // Generate swap ID
        swapId = keccak256(abi.encodePacked(
            block.chainid,
            toChain,
            msg.sender,
            toAddress,
            amount,
            block.timestamp
        ));

        // In production, this would call the cross-chain coordinator
        // For now, just emit event
        emit CrossChainTransfer(toChain, toAddress, amount, swapId);

        return swapId;
    }

    // ========== Privacy Extension ==========
    function transferPrivate(
        bytes calldata stealthAddress,
        uint256 amount,
        bytes calldata ringSignature
    ) external override notFrozen(msg.sender) returns (bool) {
        require(_balances[msg.sender] >= amount, "WEB30: insufficient balance");

        // In production, verify ring signature
        // For now, just burn from sender (receiver identity hidden)
        _balances[msg.sender] -= amount;

        emit PrivateTransfer(stealthAddress, amount);
        return true;
    }

    // ========== Governance Extension ==========
    function propose(bytes calldata proposalData) external override returns (uint256 proposalId) {
        proposalId = ++_proposalCount;

        _proposals[proposalId] = Proposal({
            proposer: msg.sender,
            title: "",
            description: "",
            startTime: block.timestamp,
            endTime: block.timestamp + 7 days,
            votesFor: 0,
            votesAgainst: 0,
            status: ProposalStatus.Active,
            actions: proposalData
        });

        emit ProposalCreated(proposalId, msg.sender);
        return proposalId;
    }

    function vote(uint256 proposalId, bool support, uint256 voteAmount) external override {
        Proposal storage proposal = _proposals[proposalId];
        require(proposal.status == ProposalStatus.Active, "WEB30: proposal not active");
        require(block.timestamp <= proposal.endTime, "WEB30: voting ended");
        require(!_hasVoted[proposalId][msg.sender], "WEB30: already voted");
        require(_balances[msg.sender] >= voteAmount, "WEB30: insufficient balance");

        _hasVoted[proposalId][msg.sender] = true;

        if (support) {
            proposal.votesFor += voteAmount;
        } else {
            proposal.votesAgainst += voteAmount;
        }

        emit Voted(proposalId, msg.sender, support, voteAmount);
    }

    function execute(uint256 proposalId) external override {
        Proposal storage proposal = _proposals[proposalId];
        require(proposal.status == ProposalStatus.Active, "WEB30: proposal not active");
        require(block.timestamp > proposal.endTime, "WEB30: voting not ended");

        if (proposal.votesFor > proposal.votesAgainst) {
            proposal.status = ProposalStatus.Succeeded;
            // Execute proposal actions
        } else {
            proposal.status = ProposalStatus.Failed;
        }
    }

    // ========== Metadata ==========
    function metadata() external view override returns (
        string memory iconUri,
        string memory description,
        string memory website
    ) {
        return (_iconUri, _description, _website);
    }

    function updateMetadata(
        string calldata iconUri,
        string calldata description,
        string calldata website
    ) external onlyOwner {
        _iconUri = iconUri;
        _description = description;
        _website = website;
    }

    // ========== Internal Functions ==========
    function _transfer(address from, address to, uint256 amount) internal {
        require(from != address(0), "WEB30: transfer from zero address");
        require(to != address(0), "WEB30: transfer to zero address");
        require(_balances[from] >= amount, "WEB30: insufficient balance");

        _balances[from] -= amount;
        _balances[to] += amount;

        emit Transfer(from, to, amount);
    }

    function _mint(address to, uint256 amount) internal {
        require(to != address(0), "WEB30: mint to zero address");

        _totalSupply += amount;
        _balances[to] += amount;

        emit Transfer(address(0), to, amount);
    }

    function _burn(address from, uint256 amount) internal {
        require(from != address(0), "WEB30: burn from zero address");
        require(_balances[from] >= amount, "WEB30: insufficient balance");

        _balances[from] -= amount;
        _totalSupply -= amount;

        emit Transfer(from, address(0), amount);
    }

    function _approve(address owner, address spender, uint256 amount) internal {
        require(owner != address(0), "WEB30: approve from zero address");
        require(spender != address(0), "WEB30: approve to zero address");

        _allowances[owner][spender] = amount;
        emit Approval(owner, spender, amount);
    }
}
