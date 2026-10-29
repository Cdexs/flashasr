import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Strings, fill, formatFingerprint } from '../i18n';
import { Overview, ServiceState, errKey, serviceState } from '../api';

// FLASH-P2b-03：服务页 —— 服务状态（含引擎 / 模型 / 大模型）与接入地址两块卡片。
// 版式沿用 voice-ime 设置界面「识别服务」页（section-title + card）。

interface Props {
  t: Strings;
  ov: Overview | null;
  loadFailed: boolean;
  refresh: () => Promise<void>;
}

const ServicePage: React.FC<Props> = ({ t, ov, loadFailed, refresh }) => {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [wanHost, setWanHost] = useState('');
  const [wanPort, setWanPort] = useState('');
  const [wanDirty, setWanDirty] = useState(false);
  const [wanSaved, setWanSaved] = useState(false);
  const [showDetail, setShowDetail] = useState(false);

  useEffect(() => {
    if (ov && !wanDirty) {
      setWanHost(ov.wan_host);
      setWanPort(ov.wan_port ? String(ov.wan_port) : '');
    }
  }, [ov, wanDirty]);

  const run = async (cmd: string, args?: Record<string, unknown>) => {
    setBusy(true);
    setError('');
    try {
      await invoke(cmd, args);
      await refresh();
    } catch (e) {
      setError(t[errKey(e)]);
    } finally {
      setBusy(false);
    }
  };

  const saveWan = async () => {
    const port = parseInt(wanPort, 10);
    await run('server_save_wan', {
      wanHost: wanHost.trim(),
      wanPort: Number.isFinite(port) && port > 0 && port < 65536 ? port : 0,
    });
    setWanDirty(false);
    setWanSaved(true);
    setTimeout(() => setWanSaved(false), 2000);
  };

  const state = serviceState(ov);
  const stateLabel: Record<ServiceState, string> = {
    stopped: t.st_stopped,
    starting: t.st_starting,
    engine_starting: t.st_engine_starting,
    load_failed: t.st_load_failed,
    loading: t.st_loading,
    paused: t.st_paused,
    running: t.st_running,
  };
  const stateColor: Record<ServiceState, string> = {
    stopped: 'var(--system-text-secondary)',
    starting: 'var(--status-warning)',
    engine_starting: 'var(--status-warning)',
    load_failed: 'var(--status-error)',
    loading: 'var(--status-warning)',
    paused: 'var(--status-warning)',
    running: 'var(--status-success)',
  };
  const stateHint: Partial<Record<ServiceState, string>> = {
    starting: t.hint_starting,
    loading: t.hint_loading,
    load_failed: t.hint_load_failed,
  };
  const hintStyle: React.CSSProperties = { fontSize: '13px', color: 'var(--system-text-tertiary)', margin: '12px 0 0', lineHeight: 1.6 };
  const lineStyle: React.CSSProperties = { fontSize: '15px', lineHeight: 1.8, margin: '0 0 8px' };
  const live = state !== 'stopped' && state !== 'starting';

  return (
    <div className="settings-page">
      <h2 className="page-title">{t.svc_title}</h2>

      {/* ① 服务状态 */}
      <section className="settings-section">
        <h3 className="section-title">{t.svc_status_section}</h3>
        <div className="card" data-testid="svc-status-card">
          {loadFailed && <p style={{ ...lineStyle, color: 'var(--status-error)' }}>{t.err_load_failed}</p>}
          <p style={lineStyle} data-testid="svc-state">
            {t.svc_status_label}
            <strong style={{ color: stateColor[state] }}>{stateLabel[state]}</strong>
          </p>
          {stateHint[state] && <p style={hintStyle}>{stateHint[state]}</p>}
          {state === 'load_failed' && ov?.load_error && (
            <div style={{ marginTop: '8px' }}>
              <button className="btn btn-link" onClick={() => setShowDetail(!showDetail)}>{t.load_error_detail}</button>
              {showDetail && (
                <pre style={{ fontSize: '12px', whiteSpace: 'pre-wrap', wordBreak: 'break-all', color: 'var(--system-text-secondary)', margin: '6px 0 0' }}>
                  {ov.load_error}
                </pre>
              )}
            </div>
          )}
          {live && (
            <p style={{ ...lineStyle, marginTop: '12px' }} data-testid="svc-llm">
              {t.llm_status_label}
              <span style={{ color: ov?.llm_configured ? 'var(--status-success)' : 'var(--system-text-secondary)' }}>
                {ov?.llm_configured ? t.llm_status_ready : t.llm_status_missing}
              </span>
            </p>
          )}

          <div className="form-row" style={{ marginTop: '12px' }}>
            <label className="form-label">{t.model_dir_label}</label>
            <span style={{ fontSize: '13px', wordBreak: 'break-all' }}>{ov?.model_dir || t.dash}</span>
          </div>
          <div style={{ display: 'flex', gap: '8px', marginTop: '8px' }}>
            <button className="btn btn-secondary btn-sm" onClick={() => invoke('open_folder', { kind: 'models' }).catch(() => {})}>{t.open_models}</button>
            <button className="btn btn-secondary btn-sm" onClick={() => invoke('open_folder', { kind: 'logs' }).catch(() => {})}>{t.open_logs}</button>
          </div>

          <div style={{ display: 'flex', gap: '8px', marginTop: '16px' }}>
            {state === 'stopped' ? (
              <button className="btn btn-primary btn-accent" data-testid="svc-start" disabled={busy} onClick={() => run('server_set_enabled', { enabled: true })}>
                {t.start}
              </button>
            ) : (
              <>
                {state === 'paused' ? (
                  <button className="btn btn-primary btn-accent" disabled={busy} onClick={() => run('server_set_paused', { paused: false })}>
                    {t.resume}
                  </button>
                ) : (
                  <button className="btn btn-secondary" data-testid="svc-pause" disabled={busy || !live} onClick={() => run('server_set_paused', { paused: true })}>
                    {t.pause}
                  </button>
                )}
                <button className="btn btn-secondary" data-testid="svc-stop" disabled={busy} onClick={() => run('server_set_enabled', { enabled: false })}>
                  {t.stop}
                </button>
              </>
            )}
          </div>
          {live && <p style={hintStyle}>{t.pause_hint}</p>}
          {error && <p style={{ ...hintStyle, color: 'var(--status-error)' }}>{error}</p>}
        </div>
      </section>

      {/* ② 接入地址 */}
      <section className="settings-section">
        <h3 className="section-title">{t.svc_network_section}</h3>
        <div className="card">
          <div className="form-row">
            <label className="form-label">{t.lan_label}</label>
            <span data-testid="svc-lan">{ov?.lan || t.lan_unknown}</span>
          </div>
          <div className="form-row">
            <label className="form-label">{t.wan_label}</label>
            <input
              className="input input-long"
              type="text"
              value={wanHost}
              placeholder={t.wan_host_placeholder}
              onChange={(e) => { setWanHost(e.target.value); setWanDirty(true); }}
            />
            <input
              className="input input-short"
              type="text"
              inputMode="numeric"
              value={wanPort}
              placeholder={ov ? String(ov.port) : t.wan_port_placeholder}
              onChange={(e) => { setWanPort(e.target.value.replace(/[^0-9]/g, '')); setWanDirty(true); }}
            />
            <button className="btn btn-secondary btn-sm" disabled={busy || !wanDirty} onClick={saveWan}>
              {wanSaved ? t.saved : t.save}
            </button>
          </div>
          <p style={hintStyle}>{fill(t.wan_hint, { port: ov?.port ?? '' })}</p>
          <div className="form-row" style={{ marginTop: '12px' }}>
            <label className="form-label">{t.fingerprint_label}</label>
            <code style={{ fontSize: '13px', wordBreak: 'break-all' }}>{ov?.fingerprint ? formatFingerprint(ov.fingerprint) : t.dash}</code>
          </div>
          <p style={hintStyle}>{t.fingerprint_hint}</p>
        </div>
      </section>
    </div>
  );
};

export default ServicePage;
