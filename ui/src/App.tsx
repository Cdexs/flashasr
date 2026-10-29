import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getTranslations } from "./i18n";
import { Overview } from "./api";
import "./styles.css";
import ServicePage from "./pages/Service";
import TerminalsPage from "./pages/Terminals";
import LlmPage from "./pages/Llm";
import AboutPage from "./pages/About";

/** 状态轮询间隔：在线状态靠轮询（控制通道一问一答，开销可忽略）。 */
const POLL_MS = 3000;

function App() {
  const t = getTranslations();
  const [activeTab, setActiveTab] = useState("service");
  const [ov, setOv] = useState<Overview | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setOv(await invoke<Overview>("server_overview"));
      setLoadFailed(false);
    } catch (e) {
      console.error("Failed to load overview:", e);
      setLoadFailed(true);
    }
  }, []);

  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, POLL_MS);
    return () => clearInterval(timer);
  }, [refresh]);

  const renderTab = () => {
    const props = { t, ov, loadFailed, refresh };
    switch (activeTab) {
      case "terminals": return <TerminalsPage {...props} />;
      case "llm": return <LlmPage t={t} />;
      case "about": return <AboutPage t={t} />;
      default: return <ServicePage {...props} />;
    }
  };

  const navItems = [
    { id: "service", label: t.nav_service },
    { id: "terminals", label: t.nav_terminals },
    { id: "llm", label: t.nav_llm },
    { id: "about", label: t.nav_about },
  ];

  const navIcons: { [key: string]: JSX.Element } = {
    service: (
      <svg className="sidebar-nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
        <rect x="3" y="3" width="18" height="7" rx="2"/><rect x="3" y="14" width="18" height="7" rx="2"/><path d="M7 6.5h.01M7 17.5h.01M11 6.5h6M11 17.5h6"/>
      </svg>
    ),
    terminals: (
      <svg className="sidebar-nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
        <rect x="5" y="2" width="14" height="20" rx="2"/><path d="M12 18h.01"/>
      </svg>
    ),
    llm: (
      <svg className="sidebar-nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
        <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z"/>
      </svg>
    ),
    about: (
      <svg className="sidebar-nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
        <circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>
      </svg>
    ),
  };

  return (
    <div className="app-container">
      <div className="sidebar">
        <div className="sidebar-title">
          <div className="sidebar-title-content">
            <svg className="sidebar-logo-icon" viewBox="0 0 24 24" fill="none" stroke="var(--brand-primary)" strokeWidth="2">
              <path d="M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3z"/>
              <path d="M19 10v2a7 7 0 01-14 0v-2"/>
            </svg>
            <span className="sidebar-title-text">{t.app_title}</span>
          </div>
        </div>
        <ul className="sidebar-nav">
          {navItems.map((item) => (
            <li
              key={item.id}
              className={`sidebar-nav-item ${activeTab === item.id ? "active" : ""}`}
              onClick={() => setActiveTab(item.id)}
            >
              {navIcons[item.id]}
              <span>{item.label}</span>
            </li>
          ))}
        </ul>
      </div>
      <div className="main-content">
        <div className="main-content-inner">{renderTab()}</div>
      </div>
    </div>
  );
}

export default App;
