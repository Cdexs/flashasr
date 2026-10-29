// 与后端 src-tauri/src/server.rs 的返回结构一一对应。

export interface TerminalRow {
  id: string;
  device_name: string;
  local: boolean;
  bound: boolean;
  created_ts: number;
  last_login_ts: number;
  last_logout_ts: number;
  net: string;
  online: boolean;
  has_voiceprint: boolean;
  week_speech_ms: number;
  week_words: number;
  total_speech_ms: number;
  total_words: number;
}

export interface Overview {
  enabled: boolean;
  port: number;
  wan_host: string;
  wan_port: number;
  running: boolean;
  paused: boolean;
  engine_online: boolean;
  local_realtime_ready: boolean;
  llm_configured: boolean;
  load_error: string;
  model_dir: string;
  lan: string;
  fingerprint: string;
  online_count: number;
  paired_count: number;
  terminals: TerminalRow[];
}

/** 服务状态（界面口径），按先后判定。 */
export type ServiceState =
  | 'stopped'
  | 'starting'
  | 'engine_starting'
  | 'load_failed'
  | 'loading'
  | 'paused'
  | 'running';

export const serviceState = (ov: Overview | null): ServiceState => {
  if (!ov || !ov.enabled) return 'stopped';
  if (!ov.running) return 'starting';
  if (!ov.engine_online) return 'engine_starting';
  if (ov.load_error) return 'load_failed';
  if (!ov.local_realtime_ready) return 'loading';
  return ov.paused ? 'paused' : 'running';
};

/** 后端错误码 ⇒ 文案键（不把内部错误原文上屏）。 */
export const errKey = (code: unknown) => {
  switch (String(code)) {
    case 'not_running': return 'err_not_running' as const;
    case 'not_found': return 'err_not_found' as const;
    case 'config_missing': return 'err_config_missing' as const;
    default: return 'err_io' as const;
  }
};
