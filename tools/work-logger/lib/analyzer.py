"""
SuperVM Work Logger - Code Analyzer
代码分析器（模块推断、Git diff 解析）
"""

import re
import subprocess
from pathlib import Path
from typing import Dict, Optional, Tuple

# 模块映射规则
MODULE_PATTERNS = [
    (r'aoem/crates/core/aoem-core', 'aoem-core'),
    (r'aoem/crates/cc/aoem-cc', 'aoem-cc'),
    (r'aoem/crates/state-kv', 'aoem-state-kv'),
    (r'src/gpu-executor', 'gpu-executor'),
    (r'src/vm-runtime', 'vm-runtime'),
    (r'src/l2-executor', 'l2-executor'),
    (r'src/defi-core', 'defi-core'),
    (r'src/domain-registry', 'domain-registry'),
    (r'plugins/evm-linker', 'evm-linker'),
    (r'plugins/bitcoin-linker', 'bitcoin-linker'),
    (r'plugins/solana-linker', 'solana-linker'),
    (r'scripts/', '脚本'),
    (r'docs/', '文档'),
    (r'tests/', '测试'),
    (r'\.github/', 'CI/CD'),
]

# 语言检测
LANGUAGE_MAP = {
    '.rs': 'Rust',
    '.ts': 'TypeScript',
    '.js': 'JavaScript',
    '.py': 'Python',
    '.md': 'Markdown',
    '.toml': 'TOML',
    '.json': 'JSON',
    '.yaml': 'YAML',
    '.yml': 'YAML',
    '.sol': 'Solidity',
    '.go': 'Go',
}

def infer_module(file_path: str) -> str:
    """推断文件所属模块"""
    normalized = file_path.replace('\\', '/')
    
    for pattern, module in MODULE_PATTERNS:
        if re.search(pattern, normalized):
            return module
    
    # 默认返回文件所在目录
    parts = normalized.split('/')
    if len(parts) > 1:
        return parts[0]
    return '未分类'

def detect_language(file_path: str) -> str:
    """检测文件语言"""
    ext = Path(file_path).suffix.lower()
    return LANGUAGE_MAP.get(ext, 'Unknown')

def parse_git_diff(file_path: str, repo_path: Path) -> Tuple[int, int]:
    """
    解析 Git diff 获取增删行数
    返回：(lines_added, lines_removed)
    """
    try:
        # 检查文件是否在 Git 中
        result = subprocess.run(
            ['git', 'diff', 'HEAD', '--', file_path],
            cwd=str(repo_path),
            capture_output=True,
            text=True,
            timeout=5
        )
        
        if result.returncode != 0:
            # 可能是新文件，尝试获取文件行数
            file_full_path = repo_path / file_path
            if file_full_path.exists():
                with open(file_full_path, 'r', encoding='utf-8', errors='ignore') as f:
                    lines = len(f.readlines())
                return lines, 0
            return 0, 0
        
        # 解析 diff 输出
        lines_added = 0
        lines_removed = 0
        
        for line in result.stdout.splitlines():
            if line.startswith('+') and not line.startswith('+++'):
                lines_added += 1
            elif line.startswith('-') and not line.startswith('---'):
                lines_removed += 1
        
        return lines_added, lines_removed
    
    except Exception as e:
        print(f"⚠️  Git diff failed for {file_path}: {e}")
        return 0, 0

def analyze_complexity(lines_added: int, lines_removed: int) -> str:
    """估算变更复杂度"""
    total = lines_added + lines_removed
    if total < 10:
        return 'LOW'
    elif total < 50:
        return 'MEDIUM'
    else:
        return 'HIGH'

def get_file_info(file_path: str, repo_path: Path) -> Dict:
    """获取文件完整信息"""
    lines_added, lines_removed = parse_git_diff(file_path, repo_path)
    
    return {
        'path': file_path,
        'module': infer_module(file_path),
        'language': detect_language(file_path),
        'lines_added': lines_added,
        'lines_removed': lines_removed,
        'complexity': analyze_complexity(lines_added, lines_removed)
    }
