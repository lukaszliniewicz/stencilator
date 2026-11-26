import '@fontsource-variable/inter/wght.css';
import '@fontsource-variable/playfair-display/wght.css';
import './app.css';

import { mount } from 'svelte';

import App from './App.svelte';
import { applyStoredTheme } from './lib/theme.svelte';

// Before the first paint, so a chosen theme does not flash the system's.
applyStoredTheme();

const target = document.getElementById('app');
if (!target) throw new Error('index.html has no #app element');

export default mount(App, { target });
