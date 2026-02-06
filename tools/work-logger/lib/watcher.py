"""
SuperVM Work Logger - File Watcher
文件监听器（主程序）
"""

import sys
import time
import signal
from pathlib import Path
from watchdog.observers import Observer
from watchdog.events import FileSystemEventHandler, FileSystemEvent

# 添加 core 目录到路径
sys.path.insert(0, str(Path(__file__).parent))

from session_manager import SessionManager
from analyzer import get_file_info, infer_module, parse_git_diff
from db_writer import WorkSessionWriter
from index_generator import update_docs_index

# 忽略的路径模式
IGNORE_PATTERNS = [
    '.git',
    'node_modules',
    'target',
    '__pycache__',
    '.vscode',
    '.idea',
    '*.log',
    '*.db',
    '*.lock',
    'cargo-target-supervm',
]

def should_ignore(path: str) -> bool:
    """判断是否应该忽略"""
    path_lower = path.lower()
    for pattern in IGNORE_PATTERNS:
        if pattern.startswith('*'):
            # 扩展名匹配
            if path_lower.endswith(pattern[1:]):
                return True
        else:
            # 路径匹配
            if pattern in path_lower:
                return True
    return False

class WorkLoggerHandler(FileSystemEventHandler):
    """工作日志处理器"""
    
    def __init__(self, session_manager: SessionManager, repo_path: Path):
        self.session_manager = session_manager
        self.repo_path = repo_path
        self.pending_changes = {}  # 用于去重
        self.last_update = time.time()
        self.docs_changed = False
    
    def _process_event(self, event: FileSystemEvent, change_type: str):
        """处理文件事件"""
        if event.is_directory:
            return
        
        # 获取相对路径
        try:
            rel_path = Path(event.src_path).relative_to(self.repo_path)
            rel_path_str = str(rel_path).replace('\\', '/')
        except ValueError:
            return
        
        # 忽略特定文件
        if should_ignore(rel_path_str):
            return

        # 忽略自动生成的索引，避免循环触发
        if rel_path_str == 'docs/INDEX.md':
            return
        
        # 记录 docs 变更
        if rel_path_str.startswith('docs/'):
            self.docs_changed = True

        # 去重处理
        self.pending_changes[rel_path_str] = {
            'type': change_type,
            'time': time.time()
        }
    
    def on_created(self, event):
        self._process_event(event, 'created')
    
    def on_modified(self, event):
        self._process_event(event, 'modified')
    
    def on_deleted(self, event):
        self._process_event(event, 'deleted')
    
    def flush_pending(self):
        """刷新待处理变更"""
        if not self.pending_changes:
            return
        
        # 处理所有待处理的变更
        for rel_path, info in self.pending_changes.items():
            try:
                file_info = get_file_info(rel_path, self.repo_path)
                self.session_manager.add_file_change(
                    rel_path,
                    info['type'],
                    file_info['lines_added'],
                    file_info['lines_removed']
                )
                print(f"📝 {info['type']}: {rel_path} (+{file_info['lines_added']} -{file_info['lines_removed']})")
            except Exception as e:
                print(f"⚠️  Failed to process {rel_path}: {e}")
        
        if self.docs_changed:
            try:
                updated = update_docs_index(self.repo_path)
                if updated:
                    print("📚 docs/INDEX.md updated")
            except Exception as e:
                print(f"⚠️  Failed to update docs index: {e}")
            self.docs_changed = False

        self.pending_changes.clear()
        self.last_update = time.time()

def main():
    """主函数"""
    # 检查参数
    if len(sys.argv) < 2:
        print("Usage: python watcher.py <repo_path>")
        sys.exit(1)
    
    repo_path = Path(sys.argv[1]).resolve()
    if not repo_path.exists():
        print(f"❌ Repository path not found: {repo_path}")
        sys.exit(1)
    
    # 初始化
    tool_root = Path(__file__).parent.parent
    storage_path = tool_root / 'data'
    session_manager = SessionManager(storage_path)
    
    # 开始会话
    session = session_manager.start_session()
    print(f"\n🚀 SuperVM Work Logger Started")
    print(f"📂 Watching: {repo_path}")
    print(f"🔑 Session ID: {session.session_id}")
    print(f"⏱️  Started at: {session.start_time.strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"\n{'='*50}")
    print("Press Ctrl+C to end session and generate report\n")
    
    # 初始化 docs/INDEX.md
    try:
        update_docs_index(repo_path)
    except Exception as e:
        print(f"⚠️  Failed to initialize docs index: {e}")

    # 创建监听器
    event_handler = WorkLoggerHandler(session_manager, repo_path)
    observer = Observer()
    observer.schedule(event_handler, str(repo_path), recursive=True)
    observer.start()
    
    # 定期刷新
    def periodic_flush():
        while True:
            time.sleep(2)  # 每 2 秒刷新一次
            if time.time() - event_handler.last_update > 1:
                event_handler.flush_pending()
                
                # 显示统计
                stats = session_manager.get_stats()
                if stats['files'] > 0:
                    print(f"\r⏱️  {stats['duration']}s | {stats['files']} files | +{stats['lines_added']} -{stats['lines_removed']} lines", end='', flush=True)
    
    # 信号处理
    def signal_handler(sig, frame):
        print(f"\n\n{'='*50}")
        print("🛑 Stopping logger...")
        observer.stop()
        observer.join()
        
        # 刷新最后的变更
        event_handler.flush_pending()
        
        # 结束会话
        completed = session_manager.end_session()
        if completed:
            print(f"\n✅ Session {completed.session_id} completed")
            print(f"📊 Duration: {format_duration(completed.get_duration())}")
            print(f"📂 Files: {len(completed.file_changes)}")
            
            # 读取用户输入的工作内容
            tool_root = Path(__file__).parent.parent
            work_note_input_file = tool_root / 'data' / 'work_note_input.json'
            work_note_data = {}
            if work_note_input_file.exists():
                try:
                    import json
                    with open(work_note_input_file, 'r', encoding='utf-8') as f:
                        work_note_data = json.load(f)
                    work_note_input_file.unlink()  # 删除临时文件
                except Exception as e:
                    print(f"⚠️  Failed to read work note input: {e}")
            
            # 推断模块信息
            module_inference = infer_module(completed.file_changes, completed.session_id)
            
            # 准备会话数据
            session_data = {
                'session_id': completed.session_id,
                'start_time': completed.start_time.isoformat(),
                'end_time': completed.end_time.isoformat() if hasattr(completed, 'end_time') and completed.end_time else None,
                'duration_seconds': completed.get_duration(),
            }
            
            # 写入数据库
            writer = WorkSessionWriter()
            success = writer.write_session(
                session_data,
                work_note_data,
                completed.file_changes,
                module_inference
            )
            
            if success:
                print(f"✅ Session recorded to database")
                # 查询最近的会话验证
                recent = writer.query_recent(1)
                if recent:
                    print(f"🔍 Latest session: {recent[0].get('session_id')}")
            else:
                print(f"⚠️  Failed to record session to database")
        
        sys.exit(0)
    
    signal.signal(signal.SIGINT, signal_handler)
    
    # 启动定期刷新
    try:
        periodic_flush()
    except KeyboardInterrupt:
        signal_handler(None, None)

def format_duration(seconds: int) -> str:
    """格式化时长"""
    hours = seconds // 3600
    minutes = (seconds % 3600) // 60
    secs = seconds % 60
    
    if hours > 0:
        return f"{hours}h {minutes}m {secs}s"
    elif minutes > 0:
        return f"{minutes}m {secs}s"
    else:
        return f"{secs}s"

if __name__ == '__main__':
    main()
