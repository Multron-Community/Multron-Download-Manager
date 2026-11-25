import { mount, type Component } from 'svelte'
import './style.css'

const hash = location.hash
const load: Promise<{ default: Component }> =
  hash === '#notice'
    ? import('./components/NoticeApp.svelte')
    : hash.startsWith('#ask=')
      ? import('./components/AskApp.svelte')
      : hash.startsWith('#confirm=')
        ? import('./components/ConfirmApp.svelte')
        : import('./App.svelte')

export default load.then(({ default: Root }) => mount(Root, { target: document.getElementById('app')! }))
