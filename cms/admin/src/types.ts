export type Role = 'admin' | 'editor';

export interface AuthUser {
  id: number;
  username: string;
  role: Role;
}

export interface AuthResponse {
  user: AuthUser;
  csrf_token: string;
}

export type BlockRegistry = Record<string, Record<string, string>>;

export interface BlockData {
  type: string;
  values: Record<string, string>;
}

export interface PluginPermissions {
  sqlite: string[];
  http: string[];
}

export interface PluginSummary {
  id: string;
  version: string;
  name: string;
  description: string;
  enabled: boolean;
  permissions: PluginPermissions;
  last_error: string | null;
  installed_at: string;
  updated_at: string;
}

export interface PluginRunResult {
  success: boolean;
  data: unknown;
  error?: string;
  code?: string;
  correlation_id: string;
}
