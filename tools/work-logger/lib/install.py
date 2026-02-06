"""
SuperVM Work Logger - Installation Script
安装脚本（设置 Git hooks、检查依赖）
"""

import sys
import subprocess
from pathlib import Path

def check_python_version():
    """检查 Python 版本"""
    version = sys.version_info
    if version.major < 3 or (version.major == 3 and version.minor < 7):
        print(f"❌ Python 3.7+ required, but found {version.major}.{version.minor}")
        return False
    print(f"✅ Python {version.major}.{version.minor}.{version.micro}")
    return True

def check_git():
    """检查 Git"""
    try:
        result = subprocess.run(['git', '--version'], capture_output=True, text=True)
        print(f"✅ {result.stdout.strip()}")
        return True
    except FileNotFoundError:
        print("❌ Git not found")
        return False

def install_watchdog():
    """安装 watchdog 库"""
    try:
        import watchdog
        print(f"✅ watchdog {watchdog.__version__} already installed")
        return True
    except ImportError:
        print("📦 Installing watchdog...")
        try:
            subprocess.run([sys.executable, '-m', 'pip', 'install', 'watchdog'], check=True)
            print("✅ watchdog installed")
            return True
        except subprocess.CalledProcessError:
            print("❌ Failed to install watchdog")
            print("   Run: pip install watchdog")
            return False

def create_git_hooks(repo_path: Path):
    """创建 Git hooks"""
    hooks_dir = repo_path / '.git' / 'hooks'
    
    if not hooks_dir.exists():
        print("⚠️  .git/hooks directory not found")
        return False
    
    # post-commit hook（提交后自动记录）
    post_commit = hooks_dir / 'post-commit'
    post_commit_content = f"""#!/bin/sh
# SuperVM Work Logger - Auto log commits

# Get commit info
COMMIT_MSG=$(git log -1 --pretty=%B)
COMMIT_HASH=$(git rev-parse --short HEAD)

# Log to work logger
echo "📝 Logged commit $COMMIT_HASH: $COMMIT_MSG"
"""
    
    with open(post_commit, 'w', encoding='utf-8', newline='\n') as f:
        f.write(post_commit_content)
    
    # Make executable (Windows: no effect, Unix: chmod +x)
    try:
        import os
        os.chmod(post_commit, 0o755)
    except:
        pass
    
    print(f"✅ Git hook created: {post_commit}")
    return True

def main():
    """主函数"""
    print("🚀 SuperVM Work Logger - Installation")
    print("="*50)
    
    # 检查 Python 版本
    if not check_python_version():
        sys.exit(1)
    
    # 检查 Git
    if not check_git():
        sys.exit(1)
    
    # 安装依赖
    if not install_watchdog():
        sys.exit(1)
    
    # 获取仓库路径
    repo_path = Path(__file__).parent.parent.parent.resolve()
    print(f"\n📂 Repository: {repo_path}")
    
    # 创建 Git hooks
    create_git_hooks(repo_path)
    
    print(f"\n{'='*50}")
    print("✅ Installation Complete!")
    print(f"\nUsage:")
    print(f"  Start:      .\\tools\\work-logger\\bin\\start.ps1")
    print(f"  Stop:       .\\tools\\work-logger\\bin\\stop.ps1")
    print(f"  Status:     .\\tools\\work-logger\\bin\\status.ps1")
    print(f"  Manual:     python tools\\work-logger\\lib\\watcher.py {repo_path}")
    print(f"\nPress Ctrl+C to stop logging and generate report.")

if __name__ == '__main__':
    main()
