import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Strings } from '../i18n';

// FLASH-P2b-03：关于页 —— 名称、版本、程序文件夹（配置 / 配对 / 声纹 / 统计 / 日志都在这里）。

interface AppInfo {
  version: string;
  data_dir: string;
}

const AboutPage: React.FC<{ t: Strings }> = ({ t }) => {
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    invoke<AppInfo>('app_info').then(setInfo).catch(() => {});
  }, []);

  return (
    <div className="settings-page">
      <section className="settings-section">
        <div className="card" style={{ textAlign: 'center', padding: '32px 24px' }}>
          <h2 className="page-title" style={{ marginBottom: '4px' }}>{t.about_title}</h2>
          <p style={{ color: 'var(--system-text-secondary)', margin: '0 0 16px' }}>{t.about_subtitle}</p>
          <p style={{ margin: 0 }} data-testid="about-version">{t.about_version} {info?.version ?? t.dash}</p>
        </div>
      </section>
      <section className="settings-section">
        <div className="card">
          <div className="form-row">
            <label className="form-label">{t.about_data_dir}</label>
            <span style={{ fontSize: '13px', wordBreak: 'break-all' }}>{info?.data_dir ?? t.dash}</span>
          </div>
          <p className="form-hint" style={{ margin: '8px 0 0' }}>{t.about_data_hint}</p>
          <button className="btn btn-secondary btn-sm" style={{ marginTop: '12px' }} onClick={() => invoke('open_folder', { kind: 'logs' }).catch(() => {})}>
            {t.open_logs}
          </button>
        </div>
      </section>
    </div>
  );
};

export default AboutPage;
