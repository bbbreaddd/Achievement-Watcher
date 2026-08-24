import { mount } from 'svelte';
import { getCurrentWindow } from '@tauri-apps/api/window';
import App from './App.svelte';
import Notification from './Notification.svelte';
import AchievementOverlay from './AchievementOverlay.svelte';
import './styles.css';

const label = getCurrentWindow().label;
const view = new URLSearchParams(window.location.search).get('view');
const isNotification = label === 'notification' || view === 'notification';
const isAchievementOverlay = label === 'achievement-overlay' || view === 'achievement-overlay';
if (isNotification) document.documentElement.classList.add('notification-view');
if (isAchievementOverlay) document.documentElement.classList.add('achievement-overlay-view');
const component = isNotification
  ? Notification
  : isAchievementOverlay
    ? AchievementOverlay
    : App;
mount(component, { target: document.getElementById('app')! });
