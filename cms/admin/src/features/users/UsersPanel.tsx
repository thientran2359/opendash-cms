import React from 'react';

interface UsersPanelProps {
  editorUsername: string;
  editorPassword: string;
  isProcessing: boolean;
  onEditorUsernameChange: (username: string) => void;
  onEditorPasswordChange: (password: string) => void;
  onCreateEditor: (e: React.FormEvent) => void;
}

export const UsersPanel: React.FC<UsersPanelProps> = ({
  editorUsername,
  editorPassword,
  isProcessing,
  onEditorUsernameChange,
  onEditorPasswordChange,
  onCreateEditor,
}) => {
  return (
    <div className="admin-section">
      <h2>Tạo tài khoản Biên tập viên</h2>
      <form onSubmit={onCreateEditor} className="form-group">
        <input
          placeholder="Tên đăng nhập editor"
          value={editorUsername}
          onChange={e => onEditorUsernameChange(e.target.value)}
          disabled={isProcessing}
        />
        <input
          type="password"
          placeholder="Mật khẩu"
          value={editorPassword}
          onChange={e => onEditorPasswordChange(e.target.value)}
          disabled={isProcessing}
        />
        <button type="submit" disabled={isProcessing}>
          Tạo Biên tập viên
        </button>
      </form>
    </div>
  );
};
