import React from 'react';
import { TextInput } from '../../components/FormPrimitives.tsx';

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
        <TextInput
          placeholder="Tên đăng nhập editor"
          value={editorUsername}
          onValueChange={onEditorUsernameChange}
          disabled={isProcessing}
        />
        <TextInput
          type="password"
          placeholder="Mật khẩu"
          value={editorPassword}
          onValueChange={onEditorPasswordChange}
          disabled={isProcessing}
        />
        <button type="submit" disabled={isProcessing}>
          Tạo Biên tập viên
        </button>
      </form>
    </div>
  );
};
