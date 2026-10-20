import { mount } from 'svelte';

import App from './App.svelte';
import './app.css';

// Sem menu de contexto do navegador dentro do app.
window.addEventListener('contextmenu', (e) => {
  if (!(e.target instanceof HTMLInputElement)) e.preventDefault();
});

export default mount(App, { target: document.getElementById('app')! });
