import React, { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Strings, TranslationKey } from '../i18n';
import { errKey } from '../api';

// FLASH-P2b-03：输出优化大模型（服务端统一配置）。版式沿用 voice-ime 设置界面「输出优化」页。
// 改动自动保存（停止输入 0.6 秒后写一次，引擎随即热重载）；改地址 / 密钥 / 模型 ⇒ 须重新测试才能启用。

interface LlmConfig {
  api_url: string;
  api_key: string;
  model: string;
  enabled: boolean;
  connectivity_verified: boolean;
}

const SAVE_DELAY_MS = 600;

const testErrKey = (code: unknown): TranslationKey => {
  switch (String(code)) {
    case 'incomplete': return 'llm_err_incomplete';
    case 'unreachable': return 'llm_err_unreachable';
    case 'auth': return 'llm_err_auth';
    case 'bad_request': return 'llm_err_bad_request';
    case 'server': return 'llm_err_server';
    case 'bad_response': return 'llm_err_bad_response';
    default: return 'err_io';
  }
};

const LlmPage: React.FC<{ t: Strings }> = ({ t }) => {
  const [llm, setLlm] = useState<LlmConfig | null>(null);
  const [loadErr, setLoadErr] = useState('');
  const [saveErr, setSaveErr] = useState('');
  const [saved, setSaved] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ msg: string; success: boolean } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    invoke<LlmConfig>('llm_get')
      .then(setLlm)
      .catch((e) => setLoadErr(t[errKey(e)]));
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, [t]);

  const save = (next: LlmConfig, immediate = false) => {
    setLlm(next);
    if (timer.current) clearTimeout(timer.current);
    const doSave = async () => {
      try {
        await invoke('llm_save', { config: next });
        setSaveErr('');
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
      } catch (e) {
        setSaveErr(t[errKey(e)]);
      }
    };
    if (immediate) doSave();
    else timer.current = setTimeout(doSave, SAVE_DELAY_MS);
  };

  if (loadErr) {
    return (
      <div className="settings-page">
        <h2 className="page-title">{t.llm_title}</h2>
        <div className="card"><p style={{ color: 'var(--status-error)', margin: 0 }}>{loadErr}</p></div>
      </div>
    );
  }
  if (!llm) return <div className="loading-state">{t.app_loading}</div>;

  // 改地址 / 密钥 / 模型 ⇒ 之前的测试结果作废，且关掉启用（未验证的配置不得生效）。
  const editField = (field: 'api_url' | 'api_key' | 'model', value: string) =>
    save({ ...llm, [field]: value, connectivity_verified: false, enabled: false });

  const canEnable =
    llm.connectivity_verified &&
    llm.api_url.trim().length > 0 &&
    llm.api_key.trim().length > 0 &&
    llm.model.trim().length > 0;

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      await invoke('llm_test', { config: llm });
      setTestResult({ msg: t.llm_test_success, success: true });
      save({ ...llm, connectivity_verified: true }, true);
    } catch (e) {
      setTestResult({ msg: `${t.llm_test_failed}${t.colon}${t[testErrKey(e)]}`, success: false });
      save({ ...llm, connectivity_verified: false, enabled: false }, true);
    } finally {
      setTesting(false);
    }
  };

  const statusClass = llm.connectivity_verified ? 'success' : '';
  const statusText = llm.connectivity_verified ? t.llm_test_success : t.llm_status_untested;

  return (
    <div className="settings-page">
      <h2 className="page-title">{t.llm_title}</h2>
      <p className="form-hint" style={{ margin: '0 0 16px', lineHeight: 1.6 }}>{t.llm_intro}</p>

      <section className="settings-section">
        <div className="toggle-row">
          <label className="toggle-switch card" style={{ border: 'none', boxShadow: 'none', padding: '8px 0', marginBottom: 0 }}>
            <input
              type="checkbox"
              checked={llm.enabled}
              onChange={(e) => {
                if (e.target.checked && !canEnable) return;
                save({ ...llm, enabled: e.target.checked }, true);
              }}
              className="toggle-input"
              data-testid="llm-enable"
            />
            <span className="toggle-track"></span>
            <span className="toggle-label" style={{ fontWeight: 600 }}>{t.llm_enable}</span>
          </label>
        </div>
        {!canEnable && (
          <p className="form-hint" style={{ color: 'var(--brand-primary)', marginTop: '4px', paddingLeft: '8px' }}>{t.llm_gate_hint}</p>
        )}
      </section>

      <section className="settings-section">
        <h3 className="section-title">{t.llm_api_config}</h3>
        <div className="card llm-api-card">
          <div className="llm-form-grid">
            <div className="llm-form-field">
              <label className="llm-form-label">{t.llm_api_url}</label>
              <input type="text" value={llm.api_url} onChange={(e) => editField('api_url', e.target.value)} className="llm-input" placeholder="https://api.deepseek.com/v1" />
            </div>
            <div className="llm-form-field">
              <label className="llm-form-label">{t.llm_api_key}</label>
              <input type="password" value={llm.api_key} onChange={(e) => editField('api_key', e.target.value)} className="llm-input" placeholder="sk-..." />
            </div>
            <div className="llm-form-field">
              <label className="llm-form-label">{t.llm_model}</label>
              <div className="llm-model-row">
                <input type="text" value={llm.model} onChange={(e) => editField('model', e.target.value)} className="llm-input" placeholder="deepseek-chat" />
                <button onClick={handleTest} disabled={testing} className="btn btn-primary llm-test-btn" data-testid="llm-test">
                  {testing ? t.llm_testing : t.llm_test_connection}
                </button>
              </div>
            </div>
            <div className="llm-form-field">
              <label className="llm-form-label"></label>
              <div className={`llm-status-bar ${statusClass}`}>
                <span className="llm-status-dot"></span>
                <span>{statusText}</span>
              </div>
            </div>
          </div>
          {testResult && (
            <div className={`llm-result-badge ${testResult.success ? 'success' : 'error'}`}>{testResult.msg}</div>
          )}
        </div>
        {saved && <p className="form-hint" style={{ color: 'var(--status-success)', marginTop: '8px' }}>{t.llm_saved_hint}</p>}
        {saveErr && <p className="form-hint" style={{ color: 'var(--status-error)', marginTop: '8px' }}>{saveErr}</p>}
      </section>
    </div>
  );
};

export default LlmPage;
