import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Strings, fill, fmtTs, formatFingerprint, formatMinutes } from '../i18n';
import { Overview, TerminalRow, errKey, serviceState } from '../api';

// FLASH-P2b-03：终端页 —— 概况（在线 / 已配对 + 添加）与终端列表（本机置顶；重置声纹、撤销、登录记录）。
// 由 voice-ime 设置界面「识别服务」页的终端两块改来。

interface Props {
  t: Strings;
  ov: Overview | null;
  loadFailed: boolean;
  refresh: () => Promise<void>;
}

interface AddedTerminal {
  terminal_id: string;
  key: string;
  lan: string;
  wan: string;
  uri: string;
  qr_svg: string;
  fingerprint: string;
}

interface SessionRow {
  login_ts: number;
  logout_ts: number | null;
}

type Confirm = { kind: 'revoke' | 'reset_vp'; row: TerminalRow };

const TerminalsPage: React.FC<Props> = ({ t, ov, refresh }) => {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [added, setAdded] = useState<AddedTerminal | null>(null);
  const [copied, setCopied] = useState(false);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [sessions, setSessions] = useState<SessionRow[]>([]);

  const running = ov?.running === true;
  const canAdd = running && serviceState(ov) !== 'paused';
  const dash = t.dash;
  const nameOf = (row: TerminalRow) => row.device_name || (row.local ? t.local_name : dash);
  const netLabel = (n: string) => (n === 'wan' ? t.net_wan : n === 'lan' ? t.net_lan : dash);
  const hintStyle: React.CSSProperties = { fontSize: '13px', color: 'var(--system-text-tertiary)', margin: '12px 0 0', lineHeight: 1.6 };
  const lineStyle: React.CSSProperties = { fontSize: '15px', lineHeight: 1.8, margin: '0 0 8px' };
  const metaStyle: React.CSSProperties = { fontSize: '13px', color: 'var(--system-text-secondary)', lineHeight: 1.8 };

  const run = async (cmd: string, args: Record<string, unknown>, done?: string) => {
    setBusy(true);
    setError('');
    setNotice('');
    try {
      await invoke(cmd, args);
      await refresh();
      if (done) {
        setNotice(done);
        setTimeout(() => setNotice(''), 2500);
      }
    } catch (e) {
      setError(t[errKey(e)]);
    } finally {
      setBusy(false);
    }
  };

  const addTerminal = async () => {
    setBusy(true);
    setError('');
    try {
      setAdded(await invoke<AddedTerminal>('server_add_terminal'));
    } catch (e) {
      setError(t[errKey(e)]);
    } finally {
      setBusy(false);
    }
  };

  const copyUri = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      console.error('Copy failed:', e);
    }
  };

  const toggleDetail = async (row: TerminalRow) => {
    if (expanded === row.id) {
      setExpanded(null);
      return;
    }
    setExpanded(row.id);
    setSessions([]);
    try {
      setSessions(await invoke<SessionRow[]>('server_sessions', { terminalId: row.id }));
    } catch (e) {
      console.error('Failed to load terminal sessions:', e);
    }
  };

  return (
    <div className="settings-page">
      <h2 className="page-title">{t.term_title}</h2>

      {/* ① 终端概况 */}
      <section className="settings-section">
        <h3 className="section-title">{t.term_summary_section}</h3>
        <div className="card" style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <span style={{ fontSize: '15px' }} data-testid="term-counts">
            {fill(t.online_paired, { online: ov?.online_count ?? 0, paired: ov?.paired_count ?? 0 })}
          </span>
          <button className="btn btn-primary btn-accent" data-testid="term-add" disabled={busy || !canAdd} onClick={addTerminal}>
            {t.add_terminal}
          </button>
        </div>
        {!running && <p style={hintStyle}>{t.add_need_running}</p>}
        {error && <p style={{ ...hintStyle, color: 'var(--status-error)' }}>{error}</p>}
        {notice && <p style={{ ...hintStyle, color: 'var(--status-success)' }}>{notice}</p>}
      </section>

      {/* ② 终端列表 */}
      <section className="settings-section">
        <h3 className="section-title">{t.list_section}</h3>
        <div className="card" data-testid="term-list">
          {(!ov || ov.terminals.length === 0) && <p style={{ ...lineStyle, color: 'var(--system-text-tertiary)' }}>{t.list_empty}</p>}
          {ov?.terminals.map((row) => (
            <div key={row.id} data-testid={`term-${row.id}`} style={{ padding: '10px 0', borderBottom: '1px solid rgba(0,0,0,0.06)' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '12px', cursor: 'pointer' }} onClick={() => toggleDetail(row)}>
                <strong style={{ fontSize: '15px' }}>{nameOf(row)}</strong>
                {row.local && <span className="badge badge-neutral">{t.local_badge}</span>}
                <span style={{ fontSize: '13px', color: row.online ? 'var(--status-success)' : 'var(--system-text-tertiary)' }}>
                  {row.online ? t.online : t.offline}
                </span>
                {!row.local && <span style={{ fontSize: '13px', color: 'var(--system-text-secondary)' }}>{netLabel(row.net)}</span>}
                <span style={{ flex: 1 }} />
                {row.has_voiceprint && (
                  <button className="btn btn-link" disabled={busy} onClick={(e) => { e.stopPropagation(); setConfirm({ kind: 'reset_vp', row }); }}>
                    {t.reset_vp}
                  </button>
                )}
                {!row.local && (
                  <button className="btn btn-link" disabled={busy} onClick={(e) => { e.stopPropagation(); setConfirm({ kind: 'revoke', row }); }}>
                    {t.revoke}
                  </button>
                )}
              </div>
              <div style={metaStyle}>
                {row.local && !row.bound ? (
                  <>{t.local_hint}{t.sep}{t.not_connected_yet}</>
                ) : (
                  <>
                    {t.voiceprint}{row.has_voiceprint ? t.vp_yes : t.vp_no}
                    {t.sep}{t.last_login}{fmtTs(row.last_login_ts, dash)}
                    {t.sep}{t.last_logout}{fmtTs(row.last_logout_ts, dash)}
                    {t.sep}{t.week}{t.duration}{formatMinutes(row.week_speech_ms)}{t.minutes}
                    {t.sep_inner}{t.words}{row.week_words}
                  </>
                )}
              </div>
              {expanded === row.id && (
                <div style={{ fontSize: '13px', lineHeight: 1.8, margin: '6px 0 0 12px' }}>
                  <div>
                    {t.total}{t.duration}{formatMinutes(row.total_speech_ms)}{t.minutes}
                    {t.sep_inner}{t.words}{row.total_words}
                  </div>
                  <div style={{ color: 'var(--system-text-secondary)' }}>{t.sessions}</div>
                  {sessions.length === 0 && <div style={{ color: 'var(--system-text-tertiary)' }}>{t.sessions_empty}</div>}
                  <ul style={{ listStyle: 'disc', margin: 0, padding: '0 0 0 20px' }}>
                    {sessions.map((s, i) => (
                      <li key={i}>
                        {fmtTs(s.login_ts, dash)}{' — '}
                        {s.logout_ts
                          ? `${fmtTs(s.logout_ts, dash)}${t.paren_open}${formatMinutes((s.logout_ts - s.login_ts) * 1000)}${t.minutes}${t.paren_close}`
                          : t.session_open}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          ))}
          <p style={hintStyle}>{t.rules_note}</p>
        </div>
      </section>

      {/* 添加终端：二维码 + 连接码 + 手输信息 */}
      {added && (
        <div className="modal-overlay">
          <div className="modal-dialog" style={{ maxWidth: '480px' }} role="dialog" data-testid="term-add-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <span className="modal-title">{t.add_title}</span>
              <button className="modal-close" onClick={() => setAdded(null)}>×</button>
            </div>
            <div className="modal-body">
              {added.qr_svg && (
                <div style={{ display: 'flex', justifyContent: 'center' }} dangerouslySetInnerHTML={{ __html: added.qr_svg }} />
              )}
              <p style={{ ...hintStyle, textAlign: 'center', margin: '4px 0 12px' }}>{t.add_qr_hint}</p>
              <div className="form-group">
                <label className="form-label">{t.conn_code}</label>
                <div style={{ display: 'flex', gap: '8px' }}>
                  <input className="input" readOnly value={added.uri} onFocus={(e) => e.target.select()} />
                  <button className="btn btn-secondary btn-sm" onClick={() => copyUri(added.uri)}>
                    {copied ? t.copied : t.copy}
                  </button>
                </div>
              </div>
              <div style={{ fontSize: '13px', lineHeight: 1.8, wordBreak: 'break-all' }}>
                <div style={{ color: 'var(--system-text-secondary)' }}>{t.manual}</div>
                <div>{t.lan_label}{t.colon}{added.lan || dash}</div>
                <div>{t.wan_label}{t.colon}{added.wan || dash}</div>
                <div>{t.key}{t.colon}<strong>{added.key}</strong></div>
                <div>{t.fingerprint_label}{t.colon}{added.fingerprint ? formatFingerprint(added.fingerprint) : dash}</div>
              </div>
              <div className="modal-hint">
                <span className="modal-hint-icon">ⓘ</span>
                <span>{t.add_note}</span>
              </div>
            </div>
            <div className="modal-footer">
              <button className="btn btn-primary btn-accent" onClick={() => setAdded(null)}>{t.close}</button>
            </div>
          </div>
        </div>
      )}

      {/* 撤销 / 重置声纹确认（不用浏览器原生 confirm） */}
      {confirm && (
        <div className="modal-overlay" onClick={() => setConfirm(null)}>
          <div className="modal-dialog modal-error" role="dialog" onClick={(e) => e.stopPropagation()}>
            <h3 className="modal-title">{confirm.kind === 'revoke' ? t.revoke : t.reset_vp}</h3>
            <div className="modal-body">
              <p>{fill(confirm.kind === 'revoke' ? t.revoke_confirm : t.reset_vp_confirm, { name: nameOf(confirm.row) })}</p>
            </div>
            <div className="modal-footer">
              <button className="btn btn-secondary" onClick={() => setConfirm(null)}>{t.cancel}</button>
              <button
                className="btn btn-danger"
                data-testid="term-confirm"
                onClick={async () => {
                  const { kind, row } = confirm;
                  setConfirm(null);
                  if (kind === 'revoke') await run('server_revoke', { terminalId: row.id });
                  else await run('server_reset_voiceprint', { terminalId: row.id }, t.reset_vp_done);
                }}
              >
                {confirm.kind === 'revoke' ? t.revoke : t.reset_vp}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default TerminalsPage;
