import { definePlugin } from '@decky/api';
import { PanelSection, PanelSectionRow, staticClasses } from '@decky/ui';
import { useEffect, useState } from 'react';
import { FaTrophy } from 'react-icons/fa';

interface UnlockEvent {
  game: string;
  displayName?: string;
  description?: string;
}

function Content() {
  const [connected, setConnected] = useState(false);
  const [latest, setLatest] = useState<UnlockEvent>();

  useEffect(() => {
    let socket: WebSocket | undefined;
    let retry: number | undefined;
    let disposed = false;
    const connect = () => {
      socket = new WebSocket('ws://127.0.0.1:8082');
      socket.onopen = () => setConnected(true);
      socket.onmessage = ({ data }) => {
        try {
          setLatest(JSON.parse(String(data)) as UnlockEvent);
        } catch {
          // Ignore messages from unrelated local WebSocket services.
        }
      };
      socket.onclose = () => {
        setConnected(false);
        if (!disposed) retry = window.setTimeout(connect, 3000);
      };
    };
    connect();
    return () => {
      disposed = true;
      window.clearTimeout(retry);
      socket?.close();
    };
  }, []);

  return (
    <PanelSection title="Companion status">
      <PanelSectionRow>
        <div>{connected ? 'Connected to Achievement Watcher' : 'Waiting for Achievement Watcher'}</div>
      </PanelSectionRow>
      {latest && (
        <PanelSectionRow>
          <div>
            <strong>{latest.displayName ?? 'Achievement unlocked'}</strong>
            <div>{latest.game}</div>
            {latest.description && <small>{latest.description}</small>}
          </div>
        </PanelSectionRow>
      )}
    </PanelSection>
  );
}

export default definePlugin(() => ({
  name: 'Achievement Watcher',
  titleView: <div className={staticClasses.Title}>Achievement Watcher</div>,
  content: <Content />,
  icon: <FaTrophy />,
}));
