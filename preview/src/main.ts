import { mount } from 'svelte';
import { getCurrentWindow } from '@tauri-apps/api/window';
import App from './App.svelte';
import Notification from './Notification.svelte';
import AchievementOverlay from './AchievementOverlay.svelte';
import './styles.css';

const label = getCurrentWindow().label;
const component = label === 'notification' ? Notification : label === 'achievement-overlay' ? AchievementOverlay : App;
mount(component, { target: document.getElementById('app')! });
