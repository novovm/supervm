"""
SuperVM Work Logger - Session Manager
会话管理器（基于 JSON 文件存储）
"""

import json
import uuid
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional

class WorkSession:
    """工作会话"""
    
    def __init__(self, session_id: Optional[str] = None):
        self.session_id = session_id or str(uuid.uuid4())[:8]
        self.start_time = datetime.now()
        self.end_time: Optional[datetime] = None
        self.paused = False
        self.pause_duration = 0  # seconds
        self.file_changes: Dict[str, Dict] = {}
        
    def add_file_change(self, file_path: str, change_type: str, lines_added: int = 0, lines_removed: int = 0):
        """添加文件变更"""
        if file_path in self.file_changes:
            # 更新已有记录
            existing = self.file_changes[file_path]
            existing['lines_added'] += lines_added
            existing['lines_removed'] += lines_removed
            existing['last_modified'] = datetime.now().isoformat()
        else:
            # 新增记录
            self.file_changes[file_path] = {
                'type': change_type,
                'lines_added': lines_added,
                'lines_removed': lines_removed,
                'first_seen': datetime.now().isoformat(),
                'last_modified': datetime.now().isoformat()
            }
    
    def end_session(self):
        """结束会话"""
        self.end_time = datetime.now()
    
    def get_duration(self) -> int:
        """获取会话时长（秒）"""
        end = self.end_time or datetime.now()
        return int((end - self.start_time).total_seconds() - self.pause_duration)
    
    def to_dict(self) -> dict:
        """转换为字典"""
        return {
            'session_id': self.session_id,
            'start_time': self.start_time.isoformat(),
            'end_time': self.end_time.isoformat() if self.end_time else None,
            'paused': self.paused,
            'pause_duration': self.pause_duration,
            'file_changes': self.file_changes
        }
    
    @staticmethod
    def from_dict(data: dict) -> 'WorkSession':
        """从字典创建"""
        session = WorkSession(data['session_id'])
        session.start_time = datetime.fromisoformat(data['start_time'])
        if data['end_time']:
            session.end_time = datetime.fromisoformat(data['end_time'])
        session.paused = data['paused']
        session.pause_duration = data['pause_duration']
        session.file_changes = data['file_changes']
        return session


class SessionManager:
    """会话管理器"""
    
    def __init__(self, storage_path: Path = None):
        if storage_path is None:
            # 默认使用 tools/work-logger/data/
            tool_root = Path(__file__).parent.parent
            storage_path = tool_root / 'data'
        self.storage_path = storage_path
        self.storage_path.mkdir(parents=True, exist_ok=True)
        self.current_session_file = storage_path / 'current_session.json'
        self.current_session: Optional[WorkSession] = None
        self._load_current_session()
    
    def _load_current_session(self):
        """加载当前会话"""
        if self.current_session_file.exists():
            try:
                with open(self.current_session_file, 'r', encoding='utf-8') as f:
                    data = json.load(f)
                    self.current_session = WorkSession.from_dict(data)
            except Exception as e:
                print(f"⚠️  Failed to load session: {e}")
    
    def _save_current_session(self):
        """保存当前会话"""
        if self.current_session:
            with open(self.current_session_file, 'w', encoding='utf-8') as f:
                json.dump(self.current_session.to_dict(), f, indent=2, ensure_ascii=False)
    
    def start_session(self) -> WorkSession:
        """开始新会话"""
        if self.current_session and not self.current_session.end_time:
            print(f"⚠️  Session {self.current_session.session_id} already active")
            return self.current_session
        
        self.current_session = WorkSession()
        self._save_current_session()
        print(f"✅ Started session {self.current_session.session_id}")
        return self.current_session
    
    def end_session(self) -> Optional[WorkSession]:
        """结束当前会话"""
        if not self.current_session:
            print("⚠️  No active session")
            return None
        
        self.current_session.end_session()
        self._save_current_session()
        
        # 归档会话
        archive_file = self.storage_path / f'session_{self.current_session.session_id}.json'
        with open(archive_file, 'w', encoding='utf-8') as f:
            json.dump(self.current_session.to_dict(), f, indent=2, ensure_ascii=False)
        
        completed = self.current_session
        self.current_session = None
        self.current_session_file.unlink(missing_ok=True)
        
        print(f"✅ Ended session {completed.session_id}")
        return completed
    
    def add_file_change(self, file_path: str, change_type: str, lines_added: int = 0, lines_removed: int = 0):
        """添加文件变更"""
        if not self.current_session:
            self.start_session()
        
        self.current_session.add_file_change(file_path, change_type, lines_added, lines_removed)
        self._save_current_session()
    
    def get_current_session(self) -> Optional[WorkSession]:
        """获取当前会话"""
        return self.current_session
    
    def get_stats(self) -> Dict:
        """获取会话统计"""
        if not self.current_session:
            return {'files': 0, 'lines_added': 0, 'lines_removed': 0, 'duration': 0}
        
        stats = {
            'files': len(self.current_session.file_changes),
            'lines_added': sum(c['lines_added'] for c in self.current_session.file_changes.values()),
            'lines_removed': sum(c['lines_removed'] for c in self.current_session.file_changes.values()),
            'duration': self.current_session.get_duration()
        }
        return stats
