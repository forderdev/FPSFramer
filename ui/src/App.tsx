import { Activity, Bell, Camera, Gamepad2, History, LayoutDashboard, LayoutTemplate, LifeBuoy, PictureInPicture2, Settings as SettingsIcon, WifiOff, X } from 'lucide-react';
import { lazy, Suspense, useEffect, type ReactNode } from 'react';
import { Banner, Button, ConfirmHost, Spinner, Switch, ToastIcon } from './components/ui';
import { act, useApp } from './lib/store';
import { navigate, useRoute } from './lib/router';
import { Wizard } from './pages/Wizard';

import { HomePage } from './pages/Home';
const OverlayPage = lazy(() => import('./pages/OverlayPage'));
const LayoutsPage = lazy(() => import('./pages/Layouts'));
const EditorPage = lazy(() => import('./editor/EditorPage'));
const MonitorPage = lazy(() => import('./pages/Monitor'));
const SessionsPage = lazy(() => import('./pages/Sessions'));
const SessionDetailPage = lazy(() => import('./pages/SessionDetail'));
const ComparePage = lazy(() => import('./pages/Compare'));
const AlertsPage = lazy(() => import('./pages/Alerts'));
const ProfilesPage = lazy(() => import('./pages/Profiles'));
const CapturesPage = lazy(() => import('./pages/Captures'));
const SettingsPage = lazy(() => import('./pages/SettingsPage'));
const HelpPage = lazy(() => import('./pages/Help'));

const NAV: { path: string; label: string; icon: ReactNode; group: string }[] = [
  { path: '/', label: 'Home', icon: <LayoutDashboard size={19} />, group: 'Monitor' },
  { path: '/monitor', label: 'Live graphs', icon: <Activity size={19} />, group: 'Monitor' },
  { path: '/sessions', label: 'Sessions', icon: <History size={19} />, group: 'Monitor' },
  { path: '/overlay', label: 'Overlay', icon: <PictureInPicture2 size={19} />, group: 'Customize' },
  { path: '/layouts', label: 'Layouts', icon: <LayoutTemplate size={19} />, group: 'Customize' },
  { path: '/profiles', label: 'Games & profiles', icon: <Gamepad2 size={19} />, group: 'Customize' },
  { path: '/alerts', label: 'Alerts', icon: <Bell size={19} />, group: 'Customize' },
  { path: '/captures', label: 'Screenshots', icon: <Camera size={19} />, group: 'More' },
  { path: '/help', label: 'Help & diagnostics', icon: <LifeBuoy size={19} />, group: 'More' },
];

function luminance(hex: string): number {
  const h = hex.replace('#', '');
  const f = h.length === 3 ? h.split('').map((c) => c + c).join('') : h.slice(0, 6);
  const n = parseInt(f, 16);
  const ch = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
}

function useAppearance() {
  const settings = useApp((s) => s.settings);
  useEffect(() => {
    if (!settings) return;
    const root = document.documentElement;
    const apply = () => {
      const t = settings.theme === 'system' ? (window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark') : settings.theme;
      root.dataset.theme = t;
    };
    apply();
    const mq = window.matchMedia('(prefers-color-scheme: light)');
    mq.addEventListener('change', apply);
    root.style.setProperty('--accent', settings.accent);
    root.style.setProperty('--accent-contrast', luminance(settings.accent) > 0.35 ? '#04141a' : '#ffffff');
    root.style.fontSize = `${16 * settings.uiScale}px`;
    root.dataset.motion = settings.reduceMotion ? 'reduce' : 'full';
    return () => mq.removeEventListener('change', apply);
  }, [settings?.theme, settings?.accent, settings?.uiScale, settings?.reduceMotion]); // eslint-disable-line react-hooks/exhaustive-deps
}

export function App() {
  const { ready, loadError, init, firstRun } = useApp();
  useAppearance();
  useEffect(() => {
    void init();
  }, [init]);

  if (loadError) {
    return (
      <div style={{ display: 'grid', placeItems: 'center', height: '100vh', padding: 24 }}>
        <div className="card" style={{ maxWidth: 480 }}>
          <div className="stack">
            <h1 className="page-title">FPSFramer couldn’t start</h1>
            <p className="muted">{loadError}</p>
            <div className="row">
              <Button variant="primary" onClick={() => window.location.reload()}>
                Try again
              </Button>
            </div>
          </div>
        </div>
      </div>
    );
  }
  if (!ready) {
    return (
      <div style={{ display: 'grid', placeItems: 'center', height: '100vh' }}>
        <div className="stack" style={{ alignItems: 'center' }}>
          <img src="/logo.svg" width={56} height={56} alt="" />
          <Spinner label="Starting FPSFramer" />
        </div>
      </div>
    );
  }
  return (
    <>
      <Shell />
      <Toasts />
      <ConfirmHost />
      {firstRun && <Wizard />}
    </>
  );
}

function Shell() {
  const route = useRoute();
  const first = route.segments[0] ?? '';
  const page = (() => {
    switch (first) {
      case '':
        return <HomePage />;
      case 'overlay':
        return <OverlayPage />;
      case 'layouts':
        return <LayoutsPage />;
      case 'editor':
        return <EditorPage id={route.segments[1] ?? ''} />;
      case 'monitor':
        return <MonitorPage />;
      case 'sessions':
        return route.segments[1] ? <SessionDetailPage id={route.segments[1]} /> : <SessionsPage />;
      case 'compare':
        return <ComparePage a={route.query.get('a') ?? ''} b={route.query.get('b') ?? ''} />;
      case 'alerts':
        return <AlertsPage />;
      case 'profiles':
        return <ProfilesPage />;
      case 'captures':
        return <CapturesPage />;
      case 'settings':
        return <SettingsPage tab={route.segments[1]} />;
      case 'help':
        return <HelpPage tab={route.segments[1]} />;
      default:
        return <HomePage />;
    }
  })();
  return (
    <div className="app">
      <Sidebar current={first === 'editor' ? 'layouts' : first === 'compare' ? 'sessions' : first} />
      <main className={`main ${first === 'editor' ? 'main-editor' : ''}`} id="main" tabIndex={-1}>
        <GlobalBanners />
        <div className="content">
        <Suspense
          fallback={
            <div className="page">
              <div className="skeleton" style={{ height: 40, width: 260 }} />
              <div className="skeleton" style={{ height: 220 }} />
            </div>
          }
        >
          {page}
        </Suspense>
        </div>
      </main>
    </div>
  );
}

function Sidebar({ current }: { current: string }) {
  const live = useApp((s) => s.live);
  const demo = useApp((s) => s.demo);
  const mode = useApp((s) => s.settings.uiMode);
  const update = useApp((s) => s.updateSettings);
  const alerts = live?.alerts.length ?? 0;
  let lastGroup = '';
  const game = live?.capture.state === 'tracking' ? live.capture.name : null;
  return (
    <nav className="side" aria-label="Main">
      <button
        type="button"
        className="btn ghost sm"
        style={{ position: 'absolute', left: -9999 }}
        onFocus={(e) => (e.currentTarget.style.left = '8px')}
        onBlur={(e) => (e.currentTarget.style.left = '-9999px')}
        onClick={() => document.getElementById('main')?.focus()}
      >
        Skip to content
      </button>
      <div className="brand">
        <img src="/logo.svg" alt="" />
        <span>FPSFramer</span>
      </div>
      {NAV.map((n) => {
        const showGroup = n.group !== lastGroup;
        lastGroup = n.group;
        const active = n.path === '/' ? current === '' : current === n.path.slice(1);
        return (
          <div key={n.path} style={{ width: '100%' }}>
            {showGroup && <div className="nav-label">{n.group}</div>}
            <a
              href={`#${n.path}`}
              className={`nav-item ${active ? 'active' : ''}`}
              aria-current={active ? 'page' : undefined}
              title={n.label}
              onClick={(e) => {
                e.preventDefault();
                navigate(n.path);
              }}
            >
              {n.icon}
              <span>{n.label}</span>
              {n.path === '/alerts' && alerts > 0 && (
                <span className="nav-badge" aria-label={`${alerts} active alerts`}>
                  {alerts}
                </span>
              )}
            </a>
          </div>
        );
      })}
      <div className="side-foot">
        <div className="side-status row" style={{ padding: '0 0.5rem', fontSize: '0.8125rem', color: 'var(--text-2)' }} role="status">
          {game ? (
            <>
              <span className="pulse-dot" aria-hidden />
              <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>Measuring {game}</span>
            </>
          ) : (
            <>
              <span style={{ width: 9, height: 9, borderRadius: '50%', background: 'var(--text-3)' }} aria-hidden />
              <span>{demo ? 'Demo mode' : 'Waiting for a game'}</span>
            </>
          )}
        </div>
        <div className="mode-switch" role="group" aria-label="Interface detail">
          <button type="button" aria-pressed={mode === 'simple'} onClick={() => void update({ uiMode: 'simple' })} title="Show the most useful options only">
            Simple
          </button>
          <button type="button" aria-pressed={mode === 'advanced'} onClick={() => void update({ uiMode: 'advanced' })} title="Show every option">
            Advanced
          </button>
        </div>
        <a
          href="#/settings"
          className={`nav-item ${current === 'settings' ? 'active' : ''}`}
          title="Settings"
          aria-current={current === 'settings' ? 'page' : undefined}
          onClick={(e) => {
            e.preventDefault();
            navigate('/settings');
          }}
        >
          <SettingsIcon size={19} />
          <span>Settings</span>
        </a>
      </div>
    </nav>
  );
}

function GlobalBanners() {
  const { connected, demo, demoGameRunning, notices, live } = useApp();
  const dismissed = useApp((s) => s.settings.dismissedTips);
  const update = useApp((s) => s.updateSettings);
  const show = notices.filter((_, i) => !dismissed.includes(`notice-${i}-${notices[i].slice(0, 24)}`));
  return (
    <>
      {(!connected || demo || show.length > 0) && (
        <div style={{ padding: '1rem 2rem 0', display: 'flex', flexDirection: 'column', gap: '0.6rem' }}>
          {!connected && (
            <Banner tone="bad" title="Connection to the engine was lost">
              <span className="row" style={{ gap: 6 }}>
                <WifiOff size={14} aria-hidden /> Trying to reconnect…
              </span>
            </Banner>
          )}
          {demo && (
            <Banner
              tone="warn"
              className="demo"
              title="Demo mode: every number here is simulated"
              actions={
                <label className="row" style={{ gap: 8 }}>
                  <span className="muted">Simulate a game</span>
                  <Switch label="Simulate a game" checked={!!demoGameRunning || !!live?.gameActive} onChange={(on) => void act('demo.setGame', { on })} />
                </label>
              }
            >
              These are not your real hardware readings. Restart FPSFramer without <code>--demo</code> to see your PC.
            </Banner>
          )}
          {show.map((n) => (
            <Banner
              key={n}
              tone="warn"
              title="A file was repaired"
              actions={
                <Button size="sm" onClick={() => void update({ dismissedTips: [...dismissed, `notice-${notices.indexOf(n)}-${n.slice(0, 24)}`] })}>
                  Got it
                </Button>
              }
            >
              {n}
            </Banner>
          ))}
        </div>
      )}
    </>
  );
}

function Toasts() {
  const toasts = useApp((s) => s.toasts);
  const dismiss = useApp((s) => s.dismissToast);
  return (
    <div className="toasts" aria-live="polite" aria-atomic="false">
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.level}`} role="status">
          <ToastIcon level={t.level} />
          <span style={{ flex: 1 }}>{t.message}</span>
          {t.undo && (
            <Button
              size="sm"
              onClick={() => {
                void t.undo?.();
                dismiss(t.id);
              }}
            >
              Undo
            </Button>
          )}
          {t.action && (
            <Button
              size="sm"
              onClick={() => {
                navigate(t.action!.route);
                dismiss(t.id);
              }}
            >
              {t.action.label}
            </Button>
          )}
          <Button variant="ghost" size="sm" icon aria-label="Dismiss" onClick={() => dismiss(t.id)}>
            <X size={15} />
          </Button>
        </div>
      ))}
    </div>
  );
}

