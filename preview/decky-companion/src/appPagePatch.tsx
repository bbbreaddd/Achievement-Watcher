import { routerHook } from '@decky/api';
import { useEffect, useState, type ReactElement } from 'react';

const APP_ROUTE = '/library/app/:appid';

interface Achievement {
  achievementId: string;
  achieved: boolean;
  displayName?: string;
  description?: string;
}

interface GameSnapshot {
  appId: string;
  gameId: string;
  name: string;
  unlocked: number;
  total: number;
  achievements: Achievement[];
}

function GamePageProbe() {
  const appId = window.location.pathname.match(/\/library\/app\/(\d+)/)?.[1];
  const [connected, setConnected] = useState(false);
  const [game, setGame] = useState<GameSnapshot | null>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (!appId) return;
    const requestId = `game-${appId}-${Date.now()}`;
    const socket = new WebSocket('ws://127.0.0.1:8082');
    socket.onopen = () => {
      setConnected(true);
      socket.send(JSON.stringify({ protocolVersion: 1, type: 'getGame', requestId, appId }));
    };
    socket.onmessage = ({ data }) => {
      try {
        const response = JSON.parse(String(data));
        if (response.type === 'game' && response.requestId === requestId) {
          setError(typeof response.error === 'string' ? response.error : undefined);
          setGame(response.game as GameSnapshot | null);
        }
      } catch {
        // Unlock broadcasts use a different message shape.
      }
    };
    socket.onclose = () => setConnected(false);
    return () => socket.close();
  }, [appId]);

  const percent = game?.total ? Math.round((game.unlocked / game.total) * 100) : 0;
  const recent = game?.achievements.filter((achievement) => achievement.achieved).slice(0, 3) ?? [];
  return (
    <aside
      style={{
        position: 'fixed',
        right: 24,
        bottom: 24,
        zIndex: 1000,
        width: 340,
        padding: '12px 16px',
        background: '#17212b',
        borderLeft: '4px solid #66c0f4',
        color: '#f5f5f5',
        boxShadow: '0 4px 16px rgba(0, 0, 0, 0.35)',
        pointerEvents: 'none',
      }}
    >
      <div style={{ fontSize: 16, fontWeight: 600 }}>Achievement Watcher</div>
      {!connected ? (
        <div style={{ marginTop: 3, color: '#acb2b8', fontSize: 12 }}>
          Open Achievement Watcher to see achievements
        </div>
      ) : error ? (
        <div style={{ marginTop: 3, color: '#e5a26f', fontSize: 12 }}>{error}</div>
      ) : game === undefined ? (
        <div style={{ marginTop: 3, color: '#acb2b8', fontSize: 12 }}>Loading achievements…</div>
      ) : game === null ? (
        <div style={{ marginTop: 3, color: '#acb2b8', fontSize: 12 }}>
          No achievements found for Steam app {appId}
        </div>
      ) : (
        <>
          <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 8, fontSize: 13 }}>
            <span>{game.name}</span>
            <span style={{ color: '#acb2b8' }}>{game.unlocked}/{game.total}</span>
          </div>
          <div style={{ height: 4, marginTop: 7, background: '#3d4852' }}>
            <div style={{ width: `${percent}%`, height: '100%', background: '#66c0f4' }} />
          </div>
          {recent.map((achievement) => (
            <div key={achievement.achievementId} style={{ marginTop: 8, fontSize: 12 }}>
              <div>{achievement.displayName ?? achievement.achievementId}</div>
              {achievement.description && (
                <div style={{ marginTop: 1, color: '#8f98a0' }}>{achievement.description}</div>
              )}
            </div>
          ))}
        </>
      )}
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
