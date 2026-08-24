import { mount } from 'svelte';
import { getCurrentWindow } from '@tauri-apps/api/window';
import App from './App.svelte';
import Notification from './Notification.svelte';
import AchievementOverlay from './AchievementOverlay.svelte';
import './styles.css';

const label = getCurrentWindow().label;
const view = new URLSearchParams(window.location.search).get('view');
const component = label === 'notification' || view === 'notification'
  ? Notification
  : label === 'achievement-overlay' || view === 'achievement-overlay'
    ? AchievementOverlay
    : App;
mount(component, { target: document.getElementById('app')! });
