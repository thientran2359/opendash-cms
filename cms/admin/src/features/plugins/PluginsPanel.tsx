import React from 'react';
import { PluginSummary } from '../../types.ts';
import { ZipUploadField } from '../../components/FormPrimitives.tsx';

interface PluginsPanelProps {
  plugins: PluginSummary[];
  selectedPluginId: string;
  pluginInputJson: string;
  pluginRunOutput: string;
  isProcessing: boolean;
  onPluginUpload: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onSelectPlugin: (id: string) => void;
  onDisablePlugin: (id: string) => void;
  onEnablePlugin: (id: string) => void;
  onPluginInputChange: (json: string) => void;
  onRunPlugin: () => void;
}

export const PluginsPanel: React.FC<PluginsPanelProps> = ({
  plugins,
  selectedPluginId,
  pluginInputJson,
  pluginRunOutput,
  isProcessing,
  onPluginUpload,
  onSelectPlugin,
  onDisablePlugin,
  onEnablePlugin,
  onPluginInputChange,
  onRunPlugin,
}) => {
  return (
    <div className="admin-section" id="plugin-management-section">
      <h2>Quản lý Plugin (Wasmi)</h2>
      <ZipUploadField
        id="plugin-zip"
        label="Cài đặt Plugin ZIP:"
        onChange={onPluginUpload}
        disabled={isProcessing}
      />

      <div className="plugin-list" style={{ marginTop: '0.75rem' }}>
        <h3>Danh sách Plugin ({plugins.length})</h3>
        {plugins.length === 0 ? (
          <p style={{ fontStyle: 'italic', color: '#666' }}>Chưa có plugin nào được cài đặt.</p>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
            {plugins.map(p => (
              <div
                key={p.id}
                style={{
                  border: p.id === selectedPluginId ? '2px solid #0066cc' : '1px solid #ccc',
                  borderRadius: '4px',
                  padding: '8px',
                  background: p.enabled ? '#fafafa' : '#f0f0f0',
                }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <strong>{p.name}</strong>
                  <span style={{ fontSize: '0.8rem', color: p.enabled ? '#0a0' : '#888' }}>
                    {p.enabled ? '● Đang bật' : '○ Đã tắt'}
                  </span>
                </div>
                <div style={{ fontSize: '0.8rem', color: '#555', marginTop: '2px' }}>
                  <code>{p.id}</code> (v{p.version})
                </div>
                {p.last_error && (
                  <div style={{ color: '#c00', fontSize: '0.75rem', marginTop: '4px' }}>
                    Lỗi gần nhất: {p.last_error}
                  </div>
                )}
                <div style={{ marginTop: '6px', display: 'flex', gap: '6px' }}>
                  {p.enabled ? (
                    <button
                      type="button"
                      className="btn-secondary"
                      style={{ padding: '2px 8px', fontSize: '0.8rem' }}
                      onClick={() => onDisablePlugin(p.id)}
                      disabled={isProcessing}
                    >
                      Tắt
                    </button>
                  ) : (
                    <button
                      type="button"
                      className="btn-secondary"
                      style={{ padding: '2px 8px', fontSize: '0.8rem' }}
                      onClick={() => onEnablePlugin(p.id)}
                      disabled={isProcessing}
                    >
                      Bật
                    </button>
                  )}
                  <button
                    type="button"
                    style={{ padding: '2px 8px', fontSize: '0.8rem' }}
                    onClick={() => onSelectPlugin(p.id)}
                    disabled={isProcessing}
                  >
                    Chọn chạy
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {selectedPluginId && (
        <div
          className="plugin-runner"
          style={{ marginTop: '1rem', borderTop: '1px solid #ddd', paddingTop: '0.75rem' }}
        >
          <h3>
            Thực thi Plugin: <code>{selectedPluginId}</code>
          </h3>
          <div className="form-group">
            <label htmlFor="plugin-input-json">Payload JSON:</label>
            <textarea
              id="plugin-input-json"
              rows={4}
              style={{ width: '100%', fontFamily: 'monospace', fontSize: '0.85rem' }}
              value={pluginInputJson}
              onChange={e => onPluginInputChange(e.target.value)}
              placeholder='{"text": "Dữ liệu mẫu"}'
            />
          </div>
          <button
            type="button"
            id="btn-run-plugin"
            onClick={onRunPlugin}
            disabled={isProcessing}
          >
            Chạy Plugin
          </button>

          {pluginRunOutput && (
            <div style={{ marginTop: '0.5rem' }}>
              <label>Kết quả:</label>
              <pre
                id="plugin-output"
                style={{
                  background: '#f8f8f8',
                  border: '1px solid #eee',
                  padding: '8px',
                  borderRadius: '4px',
                  maxHeight: '160px',
                  overflow: 'auto',
                  fontSize: '0.8rem',
                }}
              >
                {pluginRunOutput}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
