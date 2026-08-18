import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

// Svelte 5 mounts through `mount()`. `new App({ target })` is the Svelte 4 API and
// throws at runtime under Svelte 5.
const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app
