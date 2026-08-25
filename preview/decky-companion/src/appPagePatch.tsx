import { routerHook } from '@decky/api';
import type { ReactElement } from 'react';

const APP_ROUTE = '/library/app/:appid';

function GamePageProbe() {
  const appId = window.location.pathname.match(/\/library\/app\/(\d+)/)?.[1];
  return (
    <aside
      style={{
        position: 'fixed',
        right: 24,
        bottom: 24,
        zIndex: 1000,
        padding: '12px 16px',
        background: '#17212b',
        borderLeft: '4px solid #66c0f4',
        color: '#f5f5f5',
        boxShadow: '0 4px 16px rgba(0, 0, 0, 0.35)',
        pointerEvents: 'none',
      }}
    >
      <div style={{ fontSize: 16, fontWeight: 600 }}>Achievement Watcher</div>
      <div style={{ marginTop: 3, color: '#acb2b8', fontSize: 12 }}>
        Game page connection working{appId ? ` · App ${appId}` : ''}
      </div>
    </aside>
  );
}

export function patchAppPage() {
  const routePatch = routerHook.addPatch(
    APP_ROUTE,
    (props: { children?: ReactElement }) => ({
      ...props,
      children: (
        <>
          {props.children}
          <GamePageProbe />
        </>
      ),
    }),
  );

  return () => routerHook.removePatch(APP_ROUTE, routePatch);
}
